use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

use async_nats::Client;
use async_trait::async_trait;
use futures_util::{Stream, StreamExt};
use parking_lot::Mutex as SyncMutex;
use rskit_errors::{AppError, AppResult, ErrorCode};
use rskit_messaging::{BrokerConfigExt, Event, EventConsumer, Message, MessageConsumer};
use rskit_stream::SpawnedTask;
use tokio::sync::{Mutex, mpsc};
use tracing::{debug, warn};

use crate::Config;
use crate::config::{subject_for, validate_subject};
use crate::connection::connect;
use crate::error::{nats_connect_error, nats_subscribe_error};

/// NATS-backed message consumer.
pub(crate) struct NatsConsumer {
    config: Config,
    client: Mutex<Option<Client>>,
    pub(crate) sender: mpsc::Sender<Message<Vec<u8>>>,
    receiver: Mutex<mpsc::Receiver<Message<Vec<u8>>>>,
    tasks: SyncMutex<Vec<SpawnedTask>>,
    active_tasks: Arc<AtomicUsize>,
    pub(crate) subscribed: AtomicBool,
    task_finished: Arc<tokio::sync::Notify>,
}

impl NatsConsumer {
    /// Create a consumer that connects lazily when subscribing.
    pub(crate) fn new(config: Config) -> AppResult<Self> {
        config.validate()?;
        let capacity = config.subscription_buffer;
        let (sender, receiver) = mpsc::channel(capacity);
        Ok(Self {
            config,
            client: Mutex::new(None),
            sender,
            receiver: Mutex::new(receiver),
            tasks: SyncMutex::new(Vec::new()),
            active_tasks: Arc::new(AtomicUsize::new(0)),
            subscribed: AtomicBool::new(false),
            task_finished: Arc::new(tokio::sync::Notify::new()),
        })
    }

    async fn client(&self) -> AppResult<Client> {
        let mut guard = self.client.lock().await;
        if let Some(client) = guard.as_ref() {
            return Ok(client.clone());
        }
        let client = connect(&self.config).await.map_err(nats_connect_error)?;
        *guard = Some(client.clone());
        drop(guard);
        Ok(client)
    }
}

impl Drop for NatsConsumer {
    fn drop(&mut self) {
        shutdown_consumer_tasks(self.tasks.lock().drain(..).collect());
    }
}

#[async_trait]
impl MessageConsumer<Vec<u8>> for NatsConsumer {
    async fn subscribe(&self, topics: &[&str]) -> AppResult<()> {
        if topics.is_empty() {
            return Ok(());
        }
        let client = self.client().await?;
        self.subscribed.store(true, Ordering::SeqCst);
        for topic in topics {
            validate_subject("NATS subject", topic)?;
            let subject = subject_for(&self.config, topic)?;
            let subscriber = if let Some(queue_group) = self.config.base.consumer_group.as_ref() {
                client.queue_subscribe(subject, queue_group.clone()).await
            } else {
                client.subscribe(subject).await
            }
            .map_err(nats_subscribe_error)?;
            let topic_name = (*topic).to_string();
            let deliveries =
                subscriber.map(|message| (message.subject.to_string(), message.payload.to_vec()));
            let task = spawn_forwarding_task(
                topic_name,
                deliveries,
                self.sender.clone(),
                self.active_tasks.clone(),
                self.task_finished.clone(),
            );
            self.tasks.lock().push(task);
        }
        Ok(())
    }

    #[allow(clippy::significant_drop_tightening)]
    async fn recv(&self, timeout: Duration) -> AppResult<Message<Vec<u8>>> {
        if timeout.is_zero() {
            return Err(AppError::new(
                ErrorCode::InvalidInput,
                "NATS receive timeout must be greater than zero",
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
                        "NATS subscription stream closed",
                    ));
                }

                tokio::select! {
                    message = receiver.recv() => {
                        match message {
                            Some(message) => return Ok(message),
                            None => {
                                return Err(AppError::new(
                                    ErrorCode::ExternalService,
                                    "NATS subscription stream closed",
                                ));
                            }
                        }
                    }
                    () = self.task_finished.notified() => {}
                }
            }
        })
        .await
        .map_err(|error| AppError::timeout("NATS receive").with_cause(error))?
    }

    async fn close(&self) -> AppResult<()> {
        shutdown_consumer_tasks(self.tasks.lock().drain(..).collect());
        self.active_tasks.store(0, Ordering::SeqCst);
        self.client.lock().await.take();
        self.task_finished.notify_waiters();
        Ok(())
    }
}

#[async_trait]
impl EventConsumer for NatsConsumer {
    async fn subscribe(&self, topics: &[&str]) -> AppResult<()> {
        MessageConsumer::subscribe(self, topics).await
    }

    async fn recv_event(&self, timeout: Duration) -> AppResult<Event> {
        let msg = MessageConsumer::recv(self, timeout).await?;
        Event::from_json(&msg.payload)
    }
}

pub(crate) fn spawn_forwarding_task<S>(
    topic: String,
    mut deliveries: S,
    sender: mpsc::Sender<Message<Vec<u8>>>,
    active_tasks: Arc<AtomicUsize>,
    task_finished: Arc<tokio::sync::Notify>,
) -> SpawnedTask
where
    S: Stream<Item = (String, Vec<u8>)> + Unpin + Send + 'static,
{
    active_tasks.fetch_add(1, Ordering::SeqCst);
    SpawnedTask::spawn(move |task_cancellation| async move {
        loop {
            tokio::select! {
                () = task_cancellation.cancelled() => {
                    debug!(topic = %topic, "NATS consumer task shutting down");
                    break;
                }
                message = deliveries.next() => {
                    let Some((subject, payload)) = message else {
                        warn!(topic = %topic, "NATS subscription stream ended");
                        break;
                    };
                    tokio::select! {
                        () = task_cancellation.cancelled() => {
                            debug!(topic = %topic, "NATS consumer task shutting down");
                            break;
                        }
                        result = sender.send(Message::new(subject, payload)) => {
                            if result.is_err() {
                                debug!(topic = %topic, "NATS consumer receiver closed");
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
