//! NATS adapter for `rskit-messaging`.
//!
//! Connections are established lazily by producer/consumer operations; explicit
//! registration itself has no network side effects.

#![warn(missing_docs)]

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

use async_nats::Client;
use async_nats::ConnectOptions;
use async_trait::async_trait;
use bytes::Bytes;
use futures_util::StreamExt;
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

/// NATS configuration types.
pub mod config;

use crate::config::{subject_for, validate_subject};
pub use config::NatsConfig;

/// NATS-backed message producer.
pub struct NatsProducer {
    config: NatsConfig,
    client: Mutex<Option<Client>>,
}

impl NatsProducer {
    /// Create a producer that connects lazily on first use.
    pub fn new(config: NatsConfig) -> AppResult<Self> {
        config.validate()?;
        Ok(Self {
            config,
            client: Mutex::new(None),
        })
    }

    async fn client(&self) -> AppResult<Client> {
        let mut guard = self.client.lock().await;
        if let Some(client) = guard.as_ref() {
            return Ok(client.clone());
        }
        let client = connect(&self.config).await.map_err(|e| {
            AppError::new(
                ErrorCode::ExternalService,
                format!("NATS connect failed: {e}"),
            )
        })?;
        *guard = Some(client.clone());
        drop(guard);
        Ok(client)
    }
}

#[async_trait]
impl MessageProducer<Vec<u8>> for NatsProducer {
    async fn send(&self, msg: Message<Vec<u8>>) -> AppResult<()> {
        validate_subject("NATS subject", &msg.topic)?;
        let subject = subject_for(&self.config, &msg.topic)?;
        let client = self.client().await?;
        client
            .publish(subject, Bytes::from(msg.payload))
            .await
            .map_err(|e| {
                AppError::new(
                    ErrorCode::ExternalService,
                    format!("NATS publish failed: {e}"),
                )
            })?;
        debug!(topic = %msg.topic, "message sent to NATS");
        Ok(())
    }

    async fn send_batch(&self, msgs: Vec<Message<Vec<u8>>>) -> AppResult<()> {
        for msg in msgs {
            self.send(msg).await?;
        }
        Ok(())
    }

    async fn flush(&self, _timeout: Duration) -> AppResult<()> {
        let client = self.client().await?;
        client.flush().await.map_err(|e| {
            AppError::new(
                ErrorCode::ExternalService,
                format!("NATS flush failed: {e}"),
            )
        })
    }

    async fn close(&self) -> AppResult<()> {
        let client = self.client.lock().await.take();
        if let Some(client) = client {
            client.flush().await.map_err(|e| {
                AppError::new(
                    ErrorCode::ExternalService,
                    format!("NATS flush before close failed: {e}"),
                )
            })?;
        }
        Ok(())
    }
}

#[async_trait]
impl EventProducer for NatsProducer {
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

/// NATS-backed message consumer.
pub struct NatsConsumer {
    config: NatsConfig,
    client: Mutex<Option<Client>>,
    sender: mpsc::Sender<Message<Vec<u8>>>,
    receiver: Mutex<mpsc::Receiver<Message<Vec<u8>>>>,
    tasks: SyncMutex<Vec<ConsumerTask>>,
    active_tasks: Arc<AtomicUsize>,
    subscribed: AtomicBool,
    task_finished: Arc<tokio::sync::Notify>,
}

struct ConsumerTask {
    cancellation: CancellationToken,
    handle: JoinHandle<()>,
}

impl NatsConsumer {
    /// Create a consumer that connects lazily when subscribing.
    pub fn new(config: NatsConfig) -> AppResult<Self> {
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
        let client = connect(&self.config).await.map_err(|e| {
            AppError::new(
                ErrorCode::ExternalService,
                format!("NATS connect failed: {e}"),
            )
        })?;
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
        let client = self.client().await?;
        if !topics.is_empty() {
            self.subscribed.store(true, Ordering::SeqCst);
        }
        for topic in topics {
            validate_subject("NATS subject", topic)?;
            let subject = subject_for(&self.config, topic)?;
            let mut subscriber =
                if let Some(queue_group) = self.config.base.consumer_group.as_ref() {
                    client.queue_subscribe(subject, queue_group.clone()).await
                } else {
                    client.subscribe(subject).await
                }
                .map_err(|e| {
                    AppError::new(
                        ErrorCode::ExternalService,
                        format!("NATS subscribe failed: {e}"),
                    )
                })?;
            let sender = self.sender.clone();
            let cancellation = CancellationToken::new();
            let task_cancellation = cancellation.clone();
            let active_tasks = self.active_tasks.clone();
            let task_finished = self.task_finished.clone();
            active_tasks.fetch_add(1, Ordering::SeqCst);
            let topic_name = (*topic).to_string();
            let handle = tokio::spawn(async move {
                loop {
                    tokio::select! {
                        () = task_cancellation.cancelled() => {
                            debug!(topic = %topic_name, "NATS consumer task shutting down");
                            break;
                        }
                        message = subscriber.next() => {
                            let Some(message) = message else {
                                warn!(topic = %topic_name, "NATS subscription stream ended");
                                break;
                            };
                            let topic = message.subject.to_string();
                            let payload = message.payload.to_vec();
                            tokio::select! {
                                () = task_cancellation.cancelled() => {
                                    debug!(topic = %topic_name, "NATS consumer task shutting down");
                                    break;
                                }
                                result = sender.send(Message::new(topic, payload)) => {
                                    if result.is_err() {
                                        debug!(topic = %topic_name, "NATS consumer receiver closed");
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
            self.tasks.lock().push(ConsumerTask {
                cancellation,
                handle,
            });
        }
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

    async fn recv_event(&self) -> AppResult<Event> {
        let msg = MessageConsumer::recv(self).await?;
        Event::from_json(&msg.payload)
    }
}

/// Register NATS producer and consumer factories for `Vec<u8>` payloads.
pub fn register(registry: &mut MessagingRegistry<Vec<u8>>, config: NatsConfig) -> AppResult<()> {
    config.validate()?;
    if !config.base.enabled {
        return Ok(());
    }
    let adapter = config.base.adapter.clone();
    registry.register_backend(adapter, Arc::new(NatsFactory { config }))
}

struct NatsFactory {
    config: NatsConfig,
}

impl MessagingFactory<Vec<u8>> for NatsFactory {
    fn create_producer(
        &self,
        _config: &rskit_messaging::BrokerConfig,
    ) -> AppResult<Arc<dyn MessageProducer<Vec<u8>>>> {
        Ok(Arc::new(NatsProducer::new(self.config.clone())?))
    }

    fn create_consumer(
        &self,
        _config: &rskit_messaging::BrokerConfig,
    ) -> AppResult<Arc<dyn MessageConsumer<Vec<u8>>>> {
        Ok(Arc::new(NatsConsumer::new(self.config.clone())?))
    }
}

async fn connect(config: &NatsConfig) -> Result<Client, async_nats::ConnectError> {
    connect_options(config)
        .connect(config.servers.clone())
        .await
}

fn connect_options(config: &NatsConfig) -> ConnectOptions {
    let mut options = ConnectOptions::new()
        .name(config.base.name.clone())
        .connection_timeout(Duration::from_millis(config.connection_timeout))
        .request_timeout(config.base.request_timeout_duration())
        .max_reconnects(config.max_reconnects)
        .client_capacity(config.base.max_in_flight);

    let reconnect_delay = Duration::from_millis(config.reconnect_delay);
    options = options.reconnect_delay_callback(move |_| reconnect_delay);

    if let Some(token) = config.token.as_ref() {
        options = options.token(token.clone());
    }
    if let (Some(username), Some(password)) = (config.username.as_ref(), config.password.as_ref()) {
        options = options.user_and_password(username.clone(), password.clone());
    }

    options
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
    use rskit_messaging::DeliveryGuarantee;

    use super::*;

    #[test]
    fn register_adds_nats_factories_without_connecting() {
        let mut registry = MessagingRegistry::<Vec<u8>>::new();
        register(&mut registry, NatsConfig::default()).unwrap();
        assert_eq!(registry.adapters(), vec!["nats"]);
    }

    #[test]
    fn nats_config_deserializes_adapter_defaults() {
        let config: NatsConfig = serde_json::from_str("{}").unwrap();

        assert_eq!(config.base.adapter, "nats");
        assert_eq!(config.servers, vec!["tls://127.0.0.1:4222".to_string()]);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn nats_config_debug_redacts_url_credentials() {
        let config = NatsConfig {
            servers: vec!["nats://token:secret@example.test:4222".to_string()],
            subscription_buffer: 1,
            ..NatsConfig::default()
        };

        let debug = format!("{config:?}");

        assert!(debug.contains("<redacted>"));
        assert!(debug.contains("example.test:4222"));
        assert!(!debug.contains("token:secret"));
        assert!(!debug.contains("secret"));
    }

    #[test]
    fn nats_config_validate_rejects_unsupported_semantics_and_bad_auth() {
        let mut config = NatsConfig::default();
        config.base.delivery_guarantee = DeliveryGuarantee::ExactlyOnce;
        assert!(config.validate().is_err());

        config = NatsConfig::default();
        config.username = Some("user".to_string());
        assert!(config.validate().is_err());

        config = NatsConfig::default();
        config.token = Some("token".to_string());
        config.password = Some("password".to_string());
        config.username = Some("user".to_string());
        assert!(config.validate().is_err());
    }

    #[test]
    fn nats_config_rejects_plaintext_without_dev_opt_in_and_bad_names() {
        let mut config = NatsConfig {
            servers: vec!["nats://127.0.0.1:4222".to_string()],
            ..NatsConfig::default()
        };
        assert!(config.validate().is_err());
        config.allow_insecure_dev = true;
        assert!(config.validate().is_ok());

        config = NatsConfig {
            servers: vec!["tls://token:secret@example.test:4222".to_string()],
            ..NatsConfig::default()
        };
        assert!(config.validate().is_err());

        config = NatsConfig::default();
        config.base.consumer_group = Some("bad group".to_string());
        assert!(config.validate().is_err());
    }

    #[test]
    fn nats_subject_prefix_is_applied_consistently() {
        let config = NatsConfig {
            subject_prefix: "svc.".to_string(),
            ..NatsConfig::default()
        };

        assert_eq!(subject_for(&config, "events").unwrap(), "svc.events");
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
    fn nats_subject_prefix_validates_combined_subject() {
        let config = NatsConfig {
            subject_prefix: "svc.".to_string(),
            base: rskit_messaging::BrokerConfig {
                topics: vec!["bad subject".to_string()],
                ..NatsConfig::default().base
            },
            ..NatsConfig::default()
        };
        assert!(config.validate().is_err());

        let config = NatsConfig {
            subject_prefix: "svc..".to_string(),
            base: rskit_messaging::BrokerConfig {
                topics: vec!["events".to_string()],
                ..NatsConfig::default().base
            },
            ..NatsConfig::default()
        };
        assert!(config.validate().is_err());
    }

    #[tokio::test]
    async fn recv_returns_when_subscribed_tasks_have_closed() {
        let consumer = NatsConsumer::new(NatsConfig::default()).unwrap();
        consumer.subscribed.store(true, Ordering::SeqCst);

        let result = consumer.recv().await;

        assert!(result.is_err());
    }
}
