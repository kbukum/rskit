//! `RabbitMQ` adapter for `rskit-messaging`.
//!
//! The adapter uses AMQP queues named by message topic by default. Registration
//! is explicit and side-effect free; network connections are opened lazily.

#![warn(missing_docs)]

use std::collections::HashSet;
use std::fmt;
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
    MessageProducer, MessagingFactory, MessagingRegistry,
};
use tokio::sync::{Mutex, mpsc};
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::{debug, warn};

mod config;

use crate::config::{queue_for, validate_name};
pub use config::RabbitMqConfig as Config;

/// RabbitMQ-backed message producer.
pub(crate) struct RabbitMqProducer {
    config: Config,
    state: Mutex<Option<RabbitMqProducerState>>,
    declared_queues: Mutex<HashSet<String>>,
}

struct RabbitMqProducerState {
    connection: Connection,
    channel: Channel,
}

impl RabbitMqProducer {
    /// Create a producer that connects lazily on send.
    pub(crate) fn new(config: Config) -> AppResult<Self> {
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
        let channel = connection.create_channel().await.map_err(channel_failed)?;
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
                self.config.exchange.as_str().into(),
                routing_key.into(),
                BasicPublishOptions::default(),
                &msg.payload,
                BasicProperties::default(),
            )
            .await
            .map_err(publish_failed)?
            .await
            .map_err(publish_confirm_failed)?;
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
            state
                .channel
                .close(200, "closed".into())
                .await
                .map_err(channel_close_failed)?;
            state
                .connection
                .close(200, "closed".into())
                .await
                .map_err(connection_close_failed)?;
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
pub(crate) struct RabbitMqConsumer {
    config: Config,
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
pub fn register(registry: &mut MessagingRegistry<Vec<u8>>, config: Config) -> AppResult<()> {
    config.validate()?;
    if !config.base.enabled {
        return Ok(());
    }
    let adapter = config.base.adapter.clone();
    registry.register_backend(adapter, Arc::new(RabbitMqFactory { config }))
}

struct RabbitMqFactory {
    config: Config,
}

impl MessagingFactory<Vec<u8>> for RabbitMqFactory {
    fn create_producer(
        &self,
        _config: &rskit_messaging::BrokerConfig,
    ) -> AppResult<Arc<dyn MessageProducer<Vec<u8>>>> {
        Ok(Arc::new(RabbitMqProducer::new(self.config.clone())?))
    }

    fn create_consumer(
        &self,
        _config: &rskit_messaging::BrokerConfig,
    ) -> AppResult<Arc<dyn MessageConsumer<Vec<u8>>>> {
        Ok(Arc::new(RabbitMqConsumer::new(self.config.clone())?))
    }
}

fn spawn_consumer_task(
    topic: String,
    consumer: lapin::Consumer,
    sender: mpsc::Sender<Message<Vec<u8>>>,
    active_tasks: Arc<AtomicUsize>,
    task_finished: Arc<tokio::sync::Notify>,
) -> ConsumerTask {
    let deliveries = consumer
        .map(|delivery| delivery.map(|delivery| (delivery.routing_key.to_string(), delivery.data)));
    spawn_forwarding_task(topic, deliveries, sender, active_tasks, task_finished)
}

fn spawn_forwarding_task<S, E>(
    topic: String,
    mut deliveries: S,
    sender: mpsc::Sender<Message<Vec<u8>>>,
    active_tasks: Arc<AtomicUsize>,
    task_finished: Arc<tokio::sync::Notify>,
) -> ConsumerTask
where
    S: futures_util::Stream<Item = Result<(String, Vec<u8>), E>> + Unpin + Send + 'static,
    E: fmt::Display + Send + 'static,
{
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
    });
    ConsumerTask {
        cancellation,
        handle,
    }
}

async fn connect(config: &Config) -> AppResult<Connection> {
    tokio::time::timeout(
        Duration::from_millis(config.connection_timeout),
        Connection::connect(&config.uri, ConnectionProperties::default()),
    )
    .await
    .map_err(connect_timed_out)?
    .map_err(connect_failed)
}

async fn configure_qos(channel: &lapin::Channel, prefetch_count: u16) -> AppResult<()> {
    channel
        .basic_qos(prefetch_count, BasicQosOptions::default())
        .await
        .map_err(qos_configuration_failed)
}

async fn declare_queue(channel: &lapin::Channel, queue: &str, durable: bool) -> AppResult<()> {
    channel
        .queue_declare(
            queue.into(),
            QueueDeclareOptions {
                durable,
                ..QueueDeclareOptions::default()
            },
            FieldTable::default(),
        )
        .await
        .map_err(queue_declare_failed)?;
    Ok(())
}

fn rabbitmq_external_error(context: &str, error: impl fmt::Display) -> AppError {
    AppError::new(
        ErrorCode::ExternalService,
        format!("RabbitMQ {context}: {error}"),
    )
}

fn channel_failed(error: lapin::Error) -> AppError {
    rabbitmq_external_error("channel failed", error)
}

fn publish_failed(error: lapin::Error) -> AppError {
    rabbitmq_external_error("publish failed", error)
}

fn publish_confirm_failed(error: lapin::Error) -> AppError {
    rabbitmq_external_error("publish confirm failed", error)
}

fn channel_close_failed(error: lapin::Error) -> AppError {
    rabbitmq_external_error("channel close failed", error)
}

fn connection_close_failed(error: lapin::Error) -> AppError {
    rabbitmq_external_error("connection close failed", error)
}

fn consume_failed(error: lapin::Error) -> AppError {
    rabbitmq_external_error("consume failed", error)
}

fn connect_timed_out(error: impl fmt::Display) -> AppError {
    rabbitmq_external_error("connect timed out", error)
}

fn connect_failed(error: lapin::Error) -> AppError {
    rabbitmq_external_error("connect failed", error)
}

fn qos_configuration_failed(error: lapin::Error) -> AppError {
    rabbitmq_external_error("qos configuration failed", error)
}

fn queue_declare_failed(error: lapin::Error) -> AppError {
    rabbitmq_external_error("queue declare failed", error)
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
    use std::io;

    use rskit_messaging::{CommitStrategy, DeliveryGuarantee};

    use super::*;

    #[test]
    fn register_adds_rabbitmq_factories_without_connecting() {
        let mut registry = MessagingRegistry::<Vec<u8>>::new();
        register(&mut registry, Config::default()).unwrap();
        assert_eq!(registry.adapters(), vec!["rabbitmq"]);
    }

    #[test]
    fn register_skips_disabled_rabbitmq_backend() {
        let mut registry = MessagingRegistry::<Vec<u8>>::new();
        let config = Config {
            base: rskit_messaging::BrokerConfig {
                enabled: false,
                ..Config::default().base
            },
            ..Config::default()
        };

        register(&mut registry, config).unwrap();

        assert!(registry.is_empty());
    }

    #[test]
    fn register_rejects_duplicate_rabbitmq_backend() {
        let mut registry = MessagingRegistry::<Vec<u8>>::new();
        register(&mut registry, Config::default()).unwrap();

        let err = register(&mut registry, Config::default()).unwrap_err();

        assert_eq!(err.code(), ErrorCode::AlreadyExists);
    }

    #[test]
    fn rabbitmq_config_deserializes_adapter_defaults() {
        let config: Config = serde_json::from_str("{}").unwrap();

        assert_eq!(config.base.adapter, "rabbitmq");
        assert_eq!(config.uri, "amqps://127.0.0.1:5671/%2f");
        assert!(config.validate().is_ok());
    }

    #[test]
    fn rabbitmq_defaults_use_supported_auto_ack_semantics() {
        let config = Config::default();

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
        let mut config = Config::default();
        config.base.commit_strategy = CommitStrategy::Auto;
        assert!(config.effective_auto_ack());

        config.auto_ack = Some(false);
        assert!(!config.effective_auto_ack());
    }

    #[test]
    fn rabbitmq_config_rejects_unsupported_semantics() {
        let mut config = Config::default();
        config.base.delivery_guarantee = DeliveryGuarantee::ExactlyOnce;
        assert!(config.validate().is_err());

        config = Config::default();
        config.base.commit_strategy = CommitStrategy::PostHandlerSuccess;
        assert!(config.validate().is_err());

        config = Config::default();
        config.auto_ack = Some(false);
        assert!(config.validate().is_err());
    }

    #[test]
    fn rabbitmq_config_rejects_all_validation_edge_cases() {
        let mut config = Config::default();
        config.base.adapter = "other".to_string();
        assert!(config.validate().is_err());

        config = Config::default();
        config.base.retries = 1;
        assert!(config.validate().is_err());

        config = Config::default();
        config.base.dlq.enabled = true;
        assert!(config.validate().is_err());

        config = Config::default();
        config.base.request_timeout = Some(100);
        assert!(config.validate().is_err());

        config = Config::default();
        config.base.consumer_group = Some("workers".to_string());
        assert!(config.validate().is_err());

        config = Config::default();
        config.base.topics = vec!["bad queue".to_string()];
        assert!(config.validate().is_err());

        config = Config::default();
        config.uri.clear();
        assert!(config.validate().is_err());

        config = Config::default();
        config.uri = "amqps://rabbit.example.test/%2f?heartbeat=30".to_string();
        assert!(config.validate().is_err());

        config = Config::default();
        config.exchange = "bad exchange".to_string();
        assert!(config.validate().is_err());

        config = Config::default();
        config.subscription_buffer = 0;
        assert!(config.validate().is_err());

        config = Config::default();
        config.connection_timeout = 0;
        assert!(config.validate().is_err());

        config = Config::default();
        config.prefetch_count = Some(0);
        assert!(config.validate().is_err());

        config = Config::default();
        config.base.max_in_flight = usize::from(u16::MAX) + 1;
        assert!(config.effective_prefetch_count().is_err());
    }

    #[test]
    fn rabbitmq_config_rejects_plaintext_without_dev_opt_in_and_bad_names() {
        let mut config = Config {
            uri: "amqp://127.0.0.1:5672/%2f".to_string(),
            ..Config::default()
        };
        assert!(config.validate().is_err());
        config.allow_insecure_dev = true;
        assert!(config.validate().is_ok());

        config = Config {
            uri: "amqps://user:secret@example.test:5671/%2f".to_string(),
            ..Config::default()
        };
        assert!(config.validate().is_err());

        config = Config {
            queue_prefix: "bad prefix".to_string(),
            ..Config::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn rabbitmq_config_debug_redacts_uri_credentials() {
        let config = Config {
            uri: "amqp://user:password@example.test:5672/%2f".to_string(),
            ..Config::default()
        };

        let debug = format!("{config:?}");

        assert!(debug.contains("<redacted>"));
        assert!(debug.contains("example.test:5672"));
        assert!(!debug.contains("user"));
        assert!(!debug.contains("password"));
    }

    #[test]
    fn rabbitmq_queue_prefix_is_applied_and_validated() {
        let config = Config {
            queue_prefix: "svc.".to_string(),
            ..Config::default()
        };

        assert_eq!(queue_for(&config, "events").unwrap(), "svc.events");

        let config = Config {
            queue_prefix: "bad prefix.".to_string(),
            ..Config::default()
        };
        assert!(queue_for(&config, "events").is_err());
        assert!(validate_name("RabbitMQ queue", "").is_err());
        assert!(validate_name("RabbitMQ queue", &"x".repeat(250)).is_err());
    }

    #[tokio::test]
    async fn producer_and_consumer_constructors_validate_without_connecting() {
        let valid = Config::default();

        RabbitMqProducer::new(valid.clone()).unwrap();
        RabbitMqConsumer::new(valid).unwrap();

        let invalid = Config {
            subscription_buffer: 0,
            ..Config::default()
        };
        assert!(RabbitMqProducer::new(invalid.clone()).is_err());
        assert!(RabbitMqConsumer::new(invalid).is_err());
    }

    #[tokio::test]
    async fn producer_rejects_invalid_routing_key_before_connecting() {
        let producer = RabbitMqProducer::new(Config::default()).unwrap();

        let err = producer
            .send(Message::new("bad routing key", Vec::from("payload")))
            .await
            .unwrap_err();

        assert_eq!(err.code(), ErrorCode::InvalidInput);
    }

    #[tokio::test]
    async fn producer_empty_batches_flush_and_close_do_not_connect() {
        let producer = RabbitMqProducer::new(Config::default()).unwrap();

        producer.send_batch(Vec::new()).await.unwrap();
        producer.publish_batch("events", Vec::new()).await.unwrap();
        producer.flush(Duration::from_millis(1)).await.unwrap();
        producer.close().await.unwrap();

        assert!(producer.declared_queues.lock().await.is_empty());
    }

    #[tokio::test]
    async fn producer_queue_declaration_cache_tracks_declared_queues() {
        let producer = RabbitMqProducer::new(Config::default()).unwrap();

        assert!(producer.needs_queue_declare("events").await);
        producer.mark_queue_declared("events").await;
        assert!(!producer.needs_queue_declare("events").await);
        assert!(producer.needs_queue_declare("other-events").await);
    }

    #[test]
    fn rabbitmq_queue_prefix_is_applied() {
        let config = Config {
            queue_prefix: "svc.".to_string(),
            ..Config::default()
        };

        assert_eq!(queue_for(&config, "events").unwrap(), "svc.events");
    }

    #[tokio::test]
    async fn producer_skips_declaration_cache_when_exchange_routes() {
        let producer = RabbitMqProducer::new(Config {
            exchange: "events-exchange".to_string(),
            ..Config::default()
        })
        .unwrap();

        assert!(!producer.needs_queue_declare("events").await);
    }

    #[tokio::test]
    async fn producer_skips_declaration_cache_when_queue_declare_disabled() {
        let producer = RabbitMqProducer::new(Config {
            declare_queues: false,
            ..Config::default()
        })
        .unwrap();

        assert!(!producer.needs_queue_declare("events").await);
    }

    #[test]
    fn consumer_lifecycle_starts_without_resources() {
        let consumer = RabbitMqConsumer::new(Config::default()).unwrap();

        assert!(consumer.subscriptions.lock().is_empty());
    }

    #[tokio::test]
    async fn consumer_empty_subscribe_and_close_do_not_connect() {
        let consumer = RabbitMqConsumer::new(Config::default()).unwrap();

        MessageConsumer::subscribe(&consumer, &[]).await.unwrap();
        consumer.close().await.unwrap();

        assert!(!consumer.subscribed.load(Ordering::SeqCst));
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

    #[tokio::test]
    async fn shutdown_consumer_tasks_handles_empty_and_already_finished_tasks() {
        shutdown_consumer_tasks(Vec::new());

        let handle = tokio::spawn(async {});
        let task = ConsumerTask {
            cancellation: CancellationToken::new(),
            handle,
        };
        tokio::task::yield_now().await;

        shutdown_consumer_tasks(vec![task]);
    }

    #[tokio::test(start_paused = true)]
    async fn shutdown_consumer_tasks_aborts_tasks_that_ignore_cancellation() {
        let handle = tokio::spawn(futures_util::future::pending::<()>());
        let task = ConsumerTask {
            cancellation: CancellationToken::new(),
            handle,
        };

        shutdown_consumer_tasks(vec![task]);
        tokio::time::advance(Duration::from_millis(101)).await;
        tokio::task::yield_now().await;
    }

    fn lapin_io_error() -> lapin::Error {
        io::Error::other("broker failed").into()
    }

    #[test]
    fn rabbitmq_error_mappers_preserve_operation_context() {
        let cases: [(&str, AppError); 10] = [
            ("channel failed", channel_failed(lapin_io_error())),
            ("publish failed", publish_failed(lapin_io_error())),
            (
                "publish confirm failed",
                publish_confirm_failed(lapin_io_error()),
            ),
            (
                "channel close failed",
                channel_close_failed(lapin_io_error()),
            ),
            (
                "connection close failed",
                connection_close_failed(lapin_io_error()),
            ),
            ("consume failed", consume_failed(lapin_io_error())),
            ("connect timed out", connect_timed_out("deadline elapsed")),
            ("connect failed", connect_failed(lapin_io_error())),
            (
                "qos configuration failed",
                qos_configuration_failed(lapin_io_error()),
            ),
            (
                "queue declare failed",
                queue_declare_failed(lapin_io_error()),
            ),
        ];

        for (context, error) in cases {
            assert_eq!(error.code(), ErrorCode::ExternalService);
            assert!(error.message().contains(context));
        }
    }

    #[tokio::test]
    async fn forwarding_task_delivers_messages_and_notifies_when_stream_ends() {
        let (sender, mut receiver) = mpsc::channel(2);
        let active_tasks = Arc::new(AtomicUsize::new(0));
        let task_finished = Arc::new(tokio::sync::Notify::new());
        let stream = futures_util::stream::iter(vec![Ok::<_, std::io::Error>((
            "events".to_string(),
            b"payload".to_vec(),
        ))]);

        let task = spawn_forwarding_task(
            "events".to_string(),
            stream,
            sender,
            active_tasks.clone(),
            task_finished.clone(),
        );

        let message = receiver.recv().await.unwrap();
        assert_eq!(message.topic, "events");
        assert_eq!(message.payload, b"payload");
        task.handle.await.unwrap();
        assert_eq!(active_tasks.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn forwarding_task_stops_on_stream_error_receiver_close_and_cancellation() {
        let (sender, receiver) = mpsc::channel(1);
        drop(receiver);
        let active_tasks = Arc::new(AtomicUsize::new(0));
        let task_finished = Arc::new(tokio::sync::Notify::new());
        let stream = futures_util::stream::iter(vec![Ok::<_, std::io::Error>((
            "events".to_string(),
            b"payload".to_vec(),
        ))]);
        let task = spawn_forwarding_task(
            "events".to_string(),
            stream,
            sender,
            active_tasks.clone(),
            task_finished.clone(),
        );
        task.handle.await.unwrap();

        let (sender, _receiver) = mpsc::channel(1);
        let stream = futures_util::stream::iter(vec![Err::<(String, Vec<u8>), _>(
            std::io::Error::other("stream failed"),
        )]);
        let task = spawn_forwarding_task(
            "events".to_string(),
            stream,
            sender,
            active_tasks.clone(),
            task_finished.clone(),
        );
        task.handle.await.unwrap();

        let (sender, _receiver) = mpsc::channel(1);
        let stream = futures_util::stream::pending::<Result<(String, Vec<u8>), std::io::Error>>();
        let task = spawn_forwarding_task(
            "events".to_string(),
            stream,
            sender,
            active_tasks,
            task_finished.clone(),
        );
        task.cancellation.cancel();
        task.handle.await.unwrap();
    }
    #[test]
    fn rabbitmq_queue_prefix_validates_combined_queue() {
        let config = Config {
            queue_prefix: "svc.".to_string(),
            base: rskit_messaging::BrokerConfig {
                topics: vec!["bad queue".to_string()],
                ..Config::default().base
            },
            ..Config::default()
        };
        assert!(config.validate().is_err());

        let config = Config {
            queue_prefix: "x".repeat(248),
            base: rskit_messaging::BrokerConfig {
                topics: vec!["too-long".to_string()],
                ..Config::default().base
            },
            ..Config::default()
        };
        assert!(config.validate().is_err());
    }

    #[tokio::test]
    async fn recv_returns_when_subscribed_tasks_have_closed() {
        let consumer = RabbitMqConsumer::new(Config::default()).unwrap();
        consumer.subscribed.store(true, Ordering::SeqCst);

        let result = consumer.recv().await;

        assert!(result.is_err());
    }

    #[test]
    fn rabbitmq_factory_creates_lazy_producer_and_consumer() {
        let factory = RabbitMqFactory {
            config: Config::default(),
        };
        let broker = rskit_messaging::BrokerConfig::default();

        factory.create_producer(&broker).unwrap();
        factory.create_consumer(&broker).unwrap();
    }

    #[tokio::test]
    async fn event_wrappers_validate_and_decode_through_message_paths() {
        let producer = RabbitMqProducer::new(Config::default()).unwrap();
        let event = Event::new("example.created", "test");

        let err = producer
            .publish("bad routing key", event)
            .await
            .unwrap_err();
        assert_eq!(err.code(), ErrorCode::InvalidInput);

        let consumer = RabbitMqConsumer::new(Config::default()).unwrap();
        EventConsumer::subscribe(&consumer, &[]).await.unwrap();
        consumer
            .sender
            .send(Message::new("events", b"not-json".to_vec()))
            .await
            .unwrap();

        let err = consumer.recv_event().await.unwrap_err();
        assert_eq!(err.code(), ErrorCode::Internal);
    }

    #[tokio::test]
    async fn producer_and_consumer_report_fast_connection_failures() {
        let config = Config {
            uri: "amqp://127.0.0.1:1/%2f".to_string(),
            allow_insecure_dev: true,
            connection_timeout: 1,
            ..Config::default()
        };

        let producer = RabbitMqProducer::new(config.clone()).unwrap();
        let err = producer
            .send(Message::new("events", Vec::from("payload")))
            .await
            .unwrap_err();
        assert_eq!(err.code(), ErrorCode::ExternalService);

        let consumer = RabbitMqConsumer::new(config).unwrap();
        let err = MessageConsumer::subscribe(&consumer, &["events"])
            .await
            .unwrap_err();
        assert_eq!(err.code(), ErrorCode::ExternalService);
    }
}
