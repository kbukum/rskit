//! `RabbitMQ` adapter for `rskit-messaging`.
//!
//! The adapter uses AMQP queues named by message topic by default. Registration
//! is explicit and side-effect free; network connections are opened lazily.

#![warn(missing_docs)]

use std::collections::HashSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use futures_util::StreamExt;
use lapin::options::{
    BasicConsumeOptions, BasicPublishOptions, BasicQosOptions, QueueDeclareOptions,
};
use lapin::types::FieldTable;
use lapin::{BasicProperties, Channel, Connection, ConnectionProperties};
use parking_lot::Mutex as SyncMutex;
use rskit_errors::{AppError, AppResult, ErrorCode};
use rskit_messaging::{
    BrokerConfigExt, Event, EventConsumer, EventProducer, Message, MessageConsumer,
    MessageProducer, MessagingBackend, MessagingFactory, MessagingRegistry,
};
use tokio::sync::{Mutex, mpsc};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::{debug, warn};

/// `RabbitMQ` configuration types.
pub mod config;

use crate::config::{queue_for, validate_name};
pub use config::RabbitMqConfig;

/// RabbitMQ-backed message producer.
pub struct RabbitMqProducer {
    config: RabbitMqConfig,
    state: Mutex<Option<RabbitMqProducerState>>,
    declared_queues: Mutex<HashSet<String>>,
}

struct RabbitMqProducerState {
    connection: Connection,
    channel: Channel,
}

impl RabbitMqProducer {
    /// Create a producer that connects lazily on send.
    pub fn new(config: RabbitMqConfig) -> AppResult<Self> {
        config.validate()?;
        Ok(Self {
            config,
            state: Mutex::new(None),
            declared_queues: Mutex::new(HashSet::new()),
        })
    }

    async fn channel(&self) -> AppResult<Channel> {
        let mut guard = self.state.lock().await;
        if let Some(state) = guard.as_ref() {
            return Ok(state.channel.clone());
        }

        let connection = connect(&self.config).await?;
        let channel = connection.create_channel().await.map_err(|e| {
            AppError::new(
                ErrorCode::ExternalService,
                format!("RabbitMQ channel failed: {e}"),
            )
        })?;
        *guard = Some(RabbitMqProducerState {
            connection,
            channel: channel.clone(),
        });
        drop(guard);
        Ok(channel)
    }

    async fn needs_queue_declare(&self, queue: &str) -> bool {
        self.config.declare_queues
            && self.config.exchange.is_empty()
            && !self.declared_queues.lock().await.contains(queue)
    }

    async fn mark_queue_declared(&self, queue: &str) {
        self.declared_queues.lock().await.insert(queue.to_string());
    }
}

#[async_trait]
impl MessageProducer<Vec<u8>> for RabbitMqProducer {
    async fn send(&self, msg: Message<Vec<u8>>) -> AppResult<()> {
        validate_name("RabbitMQ routing key", &msg.topic)?;
        let routing_key = queue_for(&self.config, &msg.topic)?;
        let channel = self.channel().await?;
        if self.needs_queue_declare(&routing_key).await {
            declare_queue(&channel, &routing_key, self.config.durable_queues).await?;
            self.mark_queue_declared(&routing_key).await;
        }
        channel
            .basic_publish(
                &self.config.exchange,
                &routing_key,
                BasicPublishOptions::default(),
                &msg.payload,
                BasicProperties::default(),
            )
            .await
            .map_err(|e| {
                AppError::new(
                    ErrorCode::ExternalService,
                    format!("RabbitMQ publish failed: {e}"),
                )
            })?
            .await
            .map_err(|e| {
                AppError::new(
                    ErrorCode::ExternalService,
                    format!("RabbitMQ publish confirm failed: {e}"),
                )
            })?;
        debug!(topic = %msg.topic, "message sent to RabbitMQ");
        Ok(())
    }

    async fn send_batch(&self, msgs: Vec<Message<Vec<u8>>>) -> AppResult<()> {
        for msg in msgs {
            self.send(msg).await?;
        }
        Ok(())
    }

    async fn flush(&self, _timeout: Duration) -> AppResult<()> {
        Ok(())
    }

    async fn close(&self) -> AppResult<()> {
        let state = self.state.lock().await.take();
        if let Some(state) = state {
            state.channel.close(200, "closed").await.map_err(|e| {
                AppError::new(
                    ErrorCode::ExternalService,
                    format!("RabbitMQ channel close failed: {e}"),
                )
            })?;
            state.connection.close(200, "closed").await.map_err(|e| {
                AppError::new(
                    ErrorCode::ExternalService,
                    format!("RabbitMQ connection close failed: {e}"),
                )
            })?;
        }
        self.declared_queues.lock().await.clear();
        Ok(())
    }
}

#[async_trait]
impl EventProducer for RabbitMqProducer {
    async fn publish(&self, topic: &str, event: Event) -> AppResult<()> {
        self.send(Message::new(topic, event.to_json()?)).await
    }

    async fn publish_batch(&self, topic: &str, events: Vec<Event>) -> AppResult<()> {
        for event in events {
            self.publish(topic, event).await?;
        }
        Ok(())
    }
}

/// RabbitMQ-backed message consumer.
pub struct RabbitMqConsumer {
    config: RabbitMqConfig,
    sender: mpsc::Sender<Message<Vec<u8>>>,
    receiver: Mutex<mpsc::Receiver<Message<Vec<u8>>>>,
    subscriptions: SyncMutex<Vec<RabbitMqSubscription>>,
    active_tasks: Arc<AtomicUsize>,
    subscribed: AtomicBool,
    task_finished: Arc<tokio::sync::Notify>,
}

struct RabbitMqSubscription {
    _connection: Connection,
    _channels: Vec<Channel>,
    tasks: Vec<ConsumerTask>,
}

struct ConsumerTask {
    cancellation: CancellationToken,
    handle: JoinHandle<()>,
}

impl RabbitMqConsumer {
    /// Create a consumer that connects lazily on subscribe.
    pub fn new(config: RabbitMqConfig) -> AppResult<Self> {
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
            let channel = connection.create_channel().await.map_err(|e| {
                AppError::new(
                    ErrorCode::ExternalService,
                    format!("RabbitMQ channel failed: {e}"),
                )
            })?;
            if self.config.declare_queues {
                declare_queue(&channel, &queue, self.config.durable_queues).await?;
            }
            configure_qos(&channel, self.config.effective_prefetch_count()?).await?;
            let consumer = channel
                .basic_consume(
                    &queue,
                    &self.config.consumer_tag,
                    BasicConsumeOptions {
                        no_ack: self.config.effective_auto_ack(),
                        ..BasicConsumeOptions::default()
                    },
                    FieldTable::default(),
                )
                .await
                .map_err(|e| {
                    AppError::new(
                        ErrorCode::ExternalService,
                        format!("RabbitMQ consume failed: {e}"),
                    )
                })?;
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
    async fn recv(&self) -> AppResult<Message<Vec<u8>>> {
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

    async fn recv_event(&self) -> AppResult<Event> {
        let msg = MessageConsumer::recv(self).await?;
        Event::from_json(&msg.payload)
    }
}

/// Register `RabbitMQ` producer and consumer factories for `Vec<u8>` payloads.
pub fn register(
    registry: &mut MessagingRegistry<Vec<u8>>,
    config: RabbitMqConfig,
) -> AppResult<()> {
    config.validate()?;
    if !config.base.enabled {
        return Ok(());
    }
    let adapter = config.base.adapter.clone();
    registry.register_backend(adapter, Arc::new(RabbitMqFactory { config }))
}

struct RabbitMqFactory {
    config: RabbitMqConfig,
}

impl MessagingFactory<Vec<u8>> for RabbitMqFactory {
    fn create(
        &self,
        _config: &rskit_messaging::BrokerConfig,
    ) -> AppResult<MessagingBackend<Vec<u8>>> {
        Ok(MessagingBackend {
            producer: Arc::new(RabbitMqProducer::new(self.config.clone())?)
                as Arc<dyn MessageProducer<Vec<u8>>>,
            consumer: Arc::new(RabbitMqConsumer::new(self.config.clone())?)
                as Arc<dyn MessageConsumer<Vec<u8>>>,
        })
    }
}

fn spawn_consumer_task(
    topic: String,
    mut consumer: lapin::Consumer,
    sender: mpsc::Sender<Message<Vec<u8>>>,
    active_tasks: Arc<AtomicUsize>,
    task_finished: Arc<tokio::sync::Notify>,
) -> ConsumerTask {
    let cancellation = CancellationToken::new();
    let task_cancellation = cancellation.clone();
    active_tasks.fetch_add(1, Ordering::SeqCst);
    let handle = tokio::spawn(async move {
        loop {
            tokio::select! {
                () = task_cancellation.cancelled() => {
                    debug!(topic = %topic, "RabbitMQ consumer task shutting down");
                    break;
                }
                delivery = consumer.next() => {
                    let Some(delivery) = delivery else {
                        warn!(topic = %topic, "RabbitMQ consumer stream ended");
                        break;
                    };
                    let delivery = match delivery {
                        Ok(delivery) => delivery,
                        Err(error) => {
                            warn!(topic = %topic, error = %error, "RabbitMQ consumer stream error");
                            break;
                        }
                    };
                    let routing_key = delivery.routing_key.to_string();
                    let payload = delivery.data.clone();
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
    });
    ConsumerTask {
        cancellation,
        handle,
    }
}

async fn connect(config: &RabbitMqConfig) -> AppResult<Connection> {
    tokio::time::timeout(
        Duration::from_millis(config.connection_timeout),
        Connection::connect(&config.uri, ConnectionProperties::default()),
    )
    .await
    .map_err(|e| {
        AppError::new(
            ErrorCode::ExternalService,
            format!("RabbitMQ connect timed out: {e}"),
        )
    })?
    .map_err(|e| {
        AppError::new(
            ErrorCode::ExternalService,
            format!("RabbitMQ connect failed: {e}"),
        )
    })
}

async fn configure_qos(channel: &lapin::Channel, prefetch_count: u16) -> AppResult<()> {
    channel
        .basic_qos(prefetch_count, BasicQosOptions::default())
        .await
        .map_err(|e| {
            AppError::new(
                ErrorCode::ExternalService,
                format!("RabbitMQ qos configuration failed: {e}"),
            )
        })
}

async fn declare_queue(channel: &lapin::Channel, queue: &str, durable: bool) -> AppResult<()> {
    channel
        .queue_declare(
            queue,
            QueueDeclareOptions {
                durable,
                ..QueueDeclareOptions::default()
            },
            FieldTable::default(),
        )
        .await
        .map_err(|e| {
            AppError::new(
                ErrorCode::ExternalService,
                format!("RabbitMQ queue declare failed: {e}"),
            )
        })?;
    Ok(())
}

fn shutdown_consumer_tasks(tasks: Vec<ConsumerTask>) {
    if tasks.is_empty() {
        return;
    }

    for task in &tasks {
        task.cancellation.cancel();
    }

    if tokio::runtime::Handle::try_current().is_ok() {
        tokio::spawn(async move {
            for mut task in tasks {
                if task.handle.is_finished() {
                    let _ = task.handle.await;
                    continue;
                }

                if tokio::time::timeout(Duration::from_millis(100), &mut task.handle)
                    .await
                    .is_err()
                {
                    task.handle.abort();
                    let _ = task.handle.await;
                }
            }
        });
    } else {
        for task in tasks {
            task.handle.abort();
        }
    }
}

#[cfg(test)]
mod tests {
    use rskit_messaging::{CommitStrategy, DeliveryGuarantee};

    use super::*;

    #[test]
    fn register_adds_rabbitmq_factories_without_connecting() {
        let mut registry = MessagingRegistry::<Vec<u8>>::new();
        register(&mut registry, RabbitMqConfig::default()).unwrap();
        assert_eq!(registry.adapters(), vec!["rabbitmq"]);
    }

    #[test]
    fn rabbitmq_config_deserializes_adapter_defaults() {
        let config: RabbitMqConfig = serde_json::from_str("{}").unwrap();

        assert_eq!(config.base.adapter, "rabbitmq");
        assert_eq!(config.uri, "amqps://127.0.0.1:5671/%2f");
        assert!(config.validate().is_ok());
    }

    #[test]
    fn rabbitmq_defaults_use_supported_auto_ack_semantics() {
        let config = RabbitMqConfig::default();

        assert_eq!(config.base.adapter, "rabbitmq");
        assert_eq!(config.auto_ack, None);
        assert!(matches!(
            config.base.delivery_guarantee,
            DeliveryGuarantee::AtMostOnce
        ));
        assert!(matches!(config.base.commit_strategy, CommitStrategy::Auto));
        assert!(config.effective_auto_ack());
        assert_eq!(config.effective_prefetch_count().unwrap(), 1);
        assert!(config.declare_queues);
        assert!(config.durable_queues);
    }

    #[test]
    fn rabbitmq_auto_ack_follows_commit_strategy_default() {
        let mut config = RabbitMqConfig::default();
        config.base.commit_strategy = CommitStrategy::Auto;
        assert!(config.effective_auto_ack());

        config.auto_ack = Some(false);
        assert!(!config.effective_auto_ack());
    }

    #[test]
    fn rabbitmq_config_rejects_unsupported_semantics() {
        let mut config = RabbitMqConfig::default();
        config.base.delivery_guarantee = DeliveryGuarantee::ExactlyOnce;
        assert!(config.validate().is_err());

        config = RabbitMqConfig::default();
        config.base.commit_strategy = CommitStrategy::PostHandlerSuccess;
        assert!(config.validate().is_err());

        config = RabbitMqConfig::default();
        config.auto_ack = Some(false);
        assert!(config.validate().is_err());
    }

    #[test]
    fn rabbitmq_config_rejects_plaintext_without_dev_opt_in_and_bad_names() {
        let mut config = RabbitMqConfig {
            uri: "amqp://127.0.0.1:5672/%2f".to_string(),
            ..RabbitMqConfig::default()
        };
        assert!(config.validate().is_err());
        config.allow_insecure_dev = true;
        assert!(config.validate().is_ok());

        config = RabbitMqConfig {
            uri: "amqps://user:secret@example.test:5671/%2f".to_string(),
            ..RabbitMqConfig::default()
        };
        assert!(config.validate().is_err());

        config = RabbitMqConfig {
            queue_prefix: "bad prefix".to_string(),
            ..RabbitMqConfig::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn rabbitmq_config_debug_redacts_uri_credentials() {
        let config = RabbitMqConfig {
            uri: "amqp://user:password@example.test:5672/%2f".to_string(),
            ..RabbitMqConfig::default()
        };

        let debug = format!("{config:?}");

        assert!(debug.contains("<redacted>"));
        assert!(debug.contains("example.test:5672"));
        assert!(!debug.contains("user"));
        assert!(!debug.contains("password"));
    }

    #[tokio::test]
    async fn producer_queue_declaration_cache_tracks_declared_queues() {
        let producer = RabbitMqProducer::new(RabbitMqConfig::default()).unwrap();

        assert!(producer.needs_queue_declare("events").await);
        producer.mark_queue_declared("events").await;
        assert!(!producer.needs_queue_declare("events").await);
        assert!(producer.needs_queue_declare("other-events").await);
    }

    #[test]
    fn rabbitmq_queue_prefix_is_applied() {
        let config = RabbitMqConfig {
            queue_prefix: "svc.".to_string(),
            ..RabbitMqConfig::default()
        };

        assert_eq!(queue_for(&config, "events").unwrap(), "svc.events");
    }

    #[tokio::test]
    async fn producer_skips_declaration_cache_when_exchange_routes() {
        let producer = RabbitMqProducer::new(RabbitMqConfig {
            exchange: "events-exchange".to_string(),
            ..RabbitMqConfig::default()
        })
        .unwrap();

        assert!(!producer.needs_queue_declare("events").await);
    }

    #[test]
    fn consumer_lifecycle_starts_without_resources() {
        let consumer = RabbitMqConsumer::new(RabbitMqConfig::default()).unwrap();

        assert!(consumer.subscriptions.lock().is_empty());
    }

    #[tokio::test]
    async fn shutdown_consumer_tasks_requests_cancellation() {
        let cancellation = CancellationToken::new();
        let task_cancellation = cancellation.clone();
        let (sender, receiver) = tokio::sync::oneshot::channel();
        let handle = tokio::spawn(async move {
            task_cancellation.cancelled().await;
            let _ = sender.send(());
        });

        shutdown_consumer_tasks(vec![ConsumerTask {
            cancellation,
            handle,
        }]);

        tokio::time::timeout(Duration::from_secs(1), receiver)
            .await
            .unwrap()
            .unwrap();
    }
    #[test]
    fn rabbitmq_queue_prefix_validates_combined_queue() {
        let config = RabbitMqConfig {
            queue_prefix: "svc.".to_string(),
            base: rskit_messaging::BrokerConfig {
                topics: vec!["bad queue".to_string()],
                ..RabbitMqConfig::default().base
            },
            ..RabbitMqConfig::default()
        };
        assert!(config.validate().is_err());

        let config = RabbitMqConfig {
            queue_prefix: "x".repeat(248),
            base: rskit_messaging::BrokerConfig {
                topics: vec!["too-long".to_string()],
                ..RabbitMqConfig::default().base
            },
            ..RabbitMqConfig::default()
        };
        assert!(config.validate().is_err());
    }

    #[tokio::test]
    async fn recv_returns_when_subscribed_tasks_have_closed() {
        let consumer = RabbitMqConsumer::new(RabbitMqConfig::default()).unwrap();
        consumer.subscribed.store(true, Ordering::SeqCst);

        let result = consumer.recv().await;

        assert!(result.is_err());
    }
}
