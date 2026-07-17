use std::fmt;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use futures_util::StreamExt;
use lapin::options::BasicConsumeOptions;
use lapin::types::FieldTable;
use lapin::{Channel, Connection};
use parking_lot::Mutex as SyncMutex;
use rskit_errors::{AppError, AppResult, ErrorCode};
use rskit_messaging::{BrokerConfigExt, Event, EventConsumer, Message, MessageConsumer};
use rskit_stream::SpawnedTask;
use tokio::sync::{Mutex, mpsc};
use tracing::{debug, warn};

use crate::Config;
use crate::config::queue_for;
use crate::connection::{configure_qos, connect, declare_queue};
use crate::error::{channel_failed, consume_failed};

/// RabbitMQ-backed message consumer.
pub(crate) struct RabbitMqConsumer {
    config: Config,
    pub(crate) sender: mpsc::Sender<Message<Vec<u8>>>,
    receiver: Mutex<mpsc::Receiver<Message<Vec<u8>>>>,
    pub(crate) subscriptions: SyncMutex<Vec<RabbitMqSubscription>>,
    active_tasks: Arc<AtomicUsize>,
    pub(crate) subscribed: AtomicBool,
    task_finished: Arc<tokio::sync::Notify>,
}

pub(crate) struct RabbitMqSubscription {
    _connection: Connection,
    _channels: Vec<Channel>,
    tasks: Vec<SpawnedTask>,
}

impl RabbitMqConsumer {
    /// Create a consumer that connects lazily on subscribe.
    pub(crate) fn new(config: Config) -> AppResult<Self> {
        config.validate()?;
        let capacity = config.subscription_buffer;
        let (sender, receiver) = mpsc::channel(capacity);
        Ok(Self {
            config,
            sender,
            receiver: Mutex::new(receiver),
            subscriptions: SyncMutex::new(Vec::new()),
            active_tasks: Arc::new(AtomicUsize::new(0)),
            subscribed: AtomicBool::new(false),
            task_finished: Arc::new(tokio::sync::Notify::new()),
        })
    }
}

impl Drop for RabbitMqConsumer {
    fn drop(&mut self) {
        for subscription in self.subscriptions.lock().drain(..) {
            shutdown_consumer_tasks(subscription.tasks);
        }
    }
}

#[async_trait]
impl MessageConsumer<Vec<u8>> for RabbitMqConsumer {
    async fn subscribe(&self, topics: &[&str]) -> AppResult<()> {
        if topics.is_empty() {
            return Ok(());
        }
        self.subscribed.store(true, Ordering::SeqCst);

        let connection = connect(&self.config).await?;
        let mut consumers = Vec::with_capacity(topics.len());
        let mut channels = Vec::with_capacity(topics.len());

        for topic in topics {
            let queue = queue_for(&self.config, topic)?;
            let channel = connection.create_channel().await.map_err(channel_failed)?;
            if self.config.declare_queues {
                declare_queue(&channel, &queue, self.config.durable_queues).await?;
            }
            configure_qos(&channel, self.config.effective_prefetch_count()?).await?;
            let consumer = channel
                .basic_consume(
                    queue.as_str().into(),
                    self.config.consumer_tag.as_str().into(),
                    BasicConsumeOptions {
                        no_ack: self.config.effective_auto_ack(),
                        ..BasicConsumeOptions::default()
                    },
                    FieldTable::default(),
                )
                .await
                .map_err(consume_failed)?;
            consumers.push((queue, consumer));
            channels.push(channel);
        }

        let mut tasks = Vec::with_capacity(consumers.len());
        for (topic, consumer) in consumers {
            tasks.push(spawn_consumer_task(
                topic,
                consumer,
                self.sender.clone(),
                self.active_tasks.clone(),
                self.task_finished.clone(),
            ));
        }

        self.subscriptions.lock().push(RabbitMqSubscription {
            _connection: connection,
            _channels: channels,
            tasks,
        });

        Ok(())
    }

    #[allow(clippy::significant_drop_tightening)]
    async fn recv(&self, timeout: Duration) -> AppResult<Message<Vec<u8>>> {
        if timeout.is_zero() {
            return Err(AppError::new(
                ErrorCode::InvalidInput,
                "RabbitMQ receive timeout must be greater than zero",
            ));
        }
        tokio::time::timeout(timeout, async {
            let mut receiver = self.receiver.lock().await;
            loop {
                if self.subscribed.load(Ordering::SeqCst)
                    && self.active_tasks.load(Ordering::SeqCst) == 0
                    && receiver.is_empty()
                {
                    return Err(AppError::new(
                        ErrorCode::ExternalService,
                        "RabbitMQ consumer stream closed",
                    ));
                }

                tokio::select! {
                    message = receiver.recv() => {
                        match message {
                            Some(message) => return Ok(message),
                            None => {
                                return Err(AppError::new(
                                    ErrorCode::ExternalService,
                                    "RabbitMQ consumer stream closed",
                                ));
                            }
                        }
                    }
                    () = self.task_finished.notified() => {}
                }
            }
        })
        .await
        .map_err(|error| AppError::timeout("RabbitMQ receive").with_cause(error))?
    }

    async fn close(&self) -> AppResult<()> {
        for subscription in self.subscriptions.lock().drain(..) {
            shutdown_consumer_tasks(subscription.tasks);
        }
        self.active_tasks.store(0, Ordering::SeqCst);
        self.task_finished.notify_waiters();
        Ok(())
    }
}

#[async_trait]
impl EventConsumer for RabbitMqConsumer {
    async fn subscribe(&self, topics: &[&str]) -> AppResult<()> {
        MessageConsumer::subscribe(self, topics).await
    }

    async fn recv_event(&self, timeout: Duration) -> AppResult<Event> {
        let msg = MessageConsumer::recv(self, timeout).await?;
        Event::from_json(&msg.payload)
    }
}

fn spawn_consumer_task(
    topic: String,
    consumer: lapin::Consumer,
    sender: mpsc::Sender<Message<Vec<u8>>>,
    active_tasks: Arc<AtomicUsize>,
    task_finished: Arc<tokio::sync::Notify>,
) -> SpawnedTask {
    let deliveries = consumer
        .map(|delivery| delivery.map(|delivery| (delivery.routing_key.to_string(), delivery.data)));
    spawn_forwarding_task(topic, deliveries, sender, active_tasks, task_finished)
}

pub(crate) fn spawn_forwarding_task<S, E>(
    topic: String,
    mut deliveries: S,
    sender: mpsc::Sender<Message<Vec<u8>>>,
    active_tasks: Arc<AtomicUsize>,
    task_finished: Arc<tokio::sync::Notify>,
) -> SpawnedTask
where
    S: futures_util::Stream<Item = Result<(String, Vec<u8>), E>> + Unpin + Send + 'static,
    E: fmt::Display + Send + 'static,
{
    active_tasks.fetch_add(1, Ordering::SeqCst);
    SpawnedTask::spawn(move |task_cancellation| async move {
        loop {
            tokio::select! {
                () = task_cancellation.cancelled() => {
                    debug!(topic = %topic, "RabbitMQ consumer task shutting down");
                    break;
                }
                delivery = deliveries.next() => {
                    let Some(delivery) = delivery else {
                        warn!(topic = %topic, "RabbitMQ consumer stream ended");
                        break;
                    };
                    let (routing_key, payload) = match delivery {
                        Ok(delivery) => delivery,
                        Err(error) => {
                            warn!(topic = %topic, error = %error, "RabbitMQ consumer stream error");
                            break;
                        }
                    };
                    tokio::select! {
                        () = task_cancellation.cancelled() => {
                            debug!(topic = %topic, "RabbitMQ consumer task shutting down");
                            break;
                        }
                        result = sender.send(Message::new(routing_key, payload)) => {
                            if result.is_err() {
                                debug!(topic = %topic, "RabbitMQ consumer receiver closed");
                                break;
                            }
                        }
                    }
                }
            }
        }
        active_tasks.fetch_sub(1, Ordering::SeqCst);
        task_finished.notify_waiters();
    })
}

pub(crate) fn shutdown_consumer_tasks(tasks: Vec<SpawnedTask>) {
    if tasks.is_empty() {
        return;
    }

    for task in &tasks {
        task.cancel();
    }

    if tokio::runtime::Handle::try_current().is_ok() {
        tokio::spawn(async move {
            for task in tasks {
                task.shutdown(Duration::from_millis(100)).await;
            }
        });
    } else {
        for task in tasks {
            task.abort();
        }
    }
}
