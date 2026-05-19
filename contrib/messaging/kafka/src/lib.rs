//! Kafka adapter for `rskit-messaging`.
//!
//! Registration is explicit and side-effect free: call [`register`] from
//! application composition code to add Kafka factories to a [`MessagingRegistry`].

#![warn(missing_docs)]

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use rdkafka::Message as RdMessage;
use rdkafka::config::ClientConfig;
use rdkafka::consumer::{Consumer, StreamConsumer};
use rdkafka::producer::{FutureProducer, FutureRecord, Producer};
use rskit_errors::{AppError, AppResult, ErrorCode};
use rskit_messaging::{
    BrokerConfigExt, CommitStrategy, DeliveryGuarantee, Event, EventConsumer, EventProducer,
    Message, MessageConsumer, MessageProducer, MessagingBackend, MessagingFactory,
    MessagingRegistry,
};
use tokio_stream::StreamExt;
use tracing::debug;

/// Kafka configuration types.
pub mod config;

use crate::config::validate_topic;
pub use config::{Compression, KafkaConfig, OffsetReset, SecurityProtocol};

/// Kafka-backed message producer wrapping an `rdkafka` `FutureProducer`.
pub struct KafkaProducer {
    producer: FutureProducer,
}

impl KafkaProducer {
    /// Create a new `KafkaProducer` from the given configuration.
    pub fn new(config: &KafkaConfig) -> AppResult<Self> {
        config.validate()?;
        let producer: FutureProducer = producer_config(config).create().map_err(|e| {
            AppError::new(
                ErrorCode::Internal,
                format!("failed to create Kafka producer: {e}"),
            )
        })?;
        Ok(Self { producer })
    }
}

#[async_trait]
impl MessageProducer<Vec<u8>> for KafkaProducer {
    async fn send(&self, msg: Message<Vec<u8>>) -> AppResult<()> {
        validate_topic("Kafka topic", &msg.topic)?;
        let mut record = FutureRecord::to(&msg.topic).payload(&msg.payload);
        let key_ref;
        if let Some(ref key) = msg.key {
            key_ref = key.clone();
            record = record.key(&key_ref);
        }

        self.producer
            .send(record, Duration::from_secs(5))
            .await
            .map_err(|(e, _)| {
                AppError::new(
                    ErrorCode::ExternalService,
                    format!("Kafka send failed: {e}"),
                )
            })?;

        debug!(topic = %msg.topic, "message sent to Kafka");
        Ok(())
    }

    async fn send_batch(&self, msgs: Vec<Message<Vec<u8>>>) -> AppResult<()> {
        for msg in msgs {
            self.send(msg).await?;
        }
        Ok(())
    }

    async fn flush(&self, timeout: Duration) -> AppResult<()> {
        self.producer.flush(timeout).map_err(|e| {
            AppError::new(
                ErrorCode::ExternalService,
                format!("Kafka flush failed: {e}"),
            )
        })
    }
}

#[async_trait]
impl EventProducer for KafkaProducer {
    async fn publish(&self, topic: &str, event: Event) -> AppResult<()> {
        let payload = event.to_json()?;
        let msg = Message::new(topic, payload).with_key(&event.id);
        self.send(msg).await
    }

    async fn publish_batch(&self, topic: &str, events: Vec<Event>) -> AppResult<()> {
        for event in events {
            self.publish(topic, event).await?;
        }
        Ok(())
    }
}

/// Kafka-backed message consumer wrapping an `rdkafka` `StreamConsumer`.
pub struct KafkaConsumer {
    consumer: StreamConsumer,
}

impl KafkaConsumer {
    /// Create a new `KafkaConsumer` from the given configuration.
    pub fn new(config: &KafkaConfig) -> AppResult<Self> {
        config.validate()?;
        let consumer: StreamConsumer = consumer_config(config).create().map_err(|e| {
            AppError::new(
                ErrorCode::Internal,
                format!("failed to create Kafka consumer: {e}"),
            )
        })?;
        Ok(Self { consumer })
    }
}

#[async_trait]
impl MessageConsumer<Vec<u8>> for KafkaConsumer {
    async fn subscribe(&self, topics: &[&str]) -> AppResult<()> {
        for topic in topics {
            validate_topic("Kafka topic", topic)?;
        }
        self.consumer.subscribe(topics).map_err(|e| {
            AppError::new(
                ErrorCode::ExternalService,
                format!("Kafka subscribe failed: {e}"),
            )
        })
    }

    async fn recv(&self) -> AppResult<Message<Vec<u8>>> {
        let mut stream = self.consumer.stream();
        let borrowed = stream.next().await.ok_or_else(|| {
            AppError::new(
                ErrorCode::ExternalService,
                "Kafka stream ended unexpectedly",
            )
        })?;

        let borrowed = borrowed.map_err(|e| {
            AppError::new(
                ErrorCode::ExternalService,
                format!("Kafka receive error: {e}"),
            )
        })?;

        let mut msg = Message::new(
            borrowed.topic().to_string(),
            borrowed.payload().map_or_else(Vec::new, ToOwned::to_owned),
        );
        if let Some(key) = borrowed.key() {
            msg = msg.with_key(String::from_utf8_lossy(key).to_string());
        }
        msg.partition = Some(borrowed.partition());
        msg.offset = Some(borrowed.offset());

        debug!(topic = %msg.topic, partition = ?msg.partition, offset = ?msg.offset, "message received from Kafka");
        Ok(msg)
    }
}

#[async_trait]
impl EventConsumer for KafkaConsumer {
    async fn subscribe(&self, topics: &[&str]) -> AppResult<()> {
        MessageConsumer::subscribe(self, topics).await
    }

    async fn recv_event(&self) -> AppResult<Event> {
        let msg = MessageConsumer::recv(self).await?;
        Event::from_json(&msg.payload)
    }
}

/// Register Kafka producer and consumer factories for `Vec<u8>` payloads.
pub fn register(registry: &mut MessagingRegistry<Vec<u8>>, config: KafkaConfig) -> AppResult<()> {
    config.validate()?;
    if !config.base.enabled {
        return Ok(());
    }
    let adapter = config.base.adapter.clone();
    registry.register_backend(adapter, Arc::new(KafkaFactory { config }))
}

struct KafkaFactory {
    config: KafkaConfig,
}

impl MessagingFactory<Vec<u8>> for KafkaFactory {
    fn create(
        &self,
        _config: &rskit_messaging::BrokerConfig,
    ) -> AppResult<MessagingBackend<Vec<u8>>> {
        Ok(MessagingBackend {
            producer: Arc::new(KafkaProducer::new(&self.config)?)
                as Arc<dyn MessageProducer<Vec<u8>>>,
            consumer: Arc::new(KafkaConsumer::new(&self.config)?)
                as Arc<dyn MessageConsumer<Vec<u8>>>,
        })
    }
}

fn base_client_config(config: &KafkaConfig) -> ClientConfig {
    let mut cfg = ClientConfig::new();
    cfg.set("bootstrap.servers", config.brokers.join(","));
    cfg.set("security.protocol", config.security_protocol.to_string());

    if let Some(ref mechanism) = config.sasl_mechanism {
        cfg.set("sasl.mechanism", mechanism);
    }
    if let Some(ref username) = config.sasl_username {
        cfg.set("sasl.username", username);
    }
    if let Some(ref password) = config.sasl_password {
        cfg.set("sasl.password", password);
    }
    if let Some(timeout) = config.base.request_timeout {
        cfg.set("request.timeout.ms", timeout.to_string());
    }
    cfg.set("retry.backoff.ms", config.base.retry_backoff.to_string());

    cfg
}

fn producer_config(config: &KafkaConfig) -> ClientConfig {
    let mut cfg = base_client_config(config);
    let compression = match config.compression {
        Compression::None => "none",
        Compression::Gzip => "gzip",
        Compression::Snappy => "snappy",
        Compression::Lz4 => "lz4",
        Compression::Zstd => "zstd",
    };
    cfg.set("compression.type", compression);
    cfg.set("batch.size", config.batch_size.to_string());
    cfg.set("linger.ms", config.linger_ms.to_string());
    cfg.set("message.send.max.retries", config.base.retries.to_string());
    cfg.set(
        "acks",
        match config.base.delivery_guarantee {
            DeliveryGuarantee::AtMostOnce => "0",
            _ => "all",
        },
    );
    cfg.set(
        "max.in.flight.requests.per.connection",
        config.base.max_in_flight.to_string(),
    );
    cfg
}

fn consumer_config(config: &KafkaConfig) -> ClientConfig {
    let mut cfg = base_client_config(config);
    if let Some(group) = config.effective_group_id() {
        cfg.set("group.id", group);
    }
    let offset = match config.auto_offset_reset {
        OffsetReset::Latest => "latest",
        OffsetReset::Earliest => "earliest",
    };
    cfg.set("auto.offset.reset", offset);
    let auto_commit = matches!(config.base.commit_strategy, CommitStrategy::Auto);
    cfg.set("enable.auto.commit", auto_commit.to_string());
    cfg.set(
        "session.timeout.ms",
        config.session_timeout.as_millis().to_string(),
    );
    cfg
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;

    #[test]
    fn kafka_config_default_values() {
        let config = KafkaConfig::default();

        assert_eq!(config.base.adapter, "kafka");
        assert_eq!(config.brokers, vec!["localhost:9092".to_string()]);
        assert_eq!(config.base.retries, 3);
        assert_eq!(config.batch_size, 1000);
        assert_eq!(config.linger_ms, 5);
        assert_eq!(config.session_timeout, Duration::from_secs(30));
        assert_eq!(config.base.name, "default");
        assert!(config.base.enabled);
        assert_eq!(config.security_protocol, SecurityProtocol::Ssl);
        assert!(!config.allow_insecure_dev);
    }

    #[test]
    fn kafka_config_deserializes_adapter_defaults() {
        let config: KafkaConfig = serde_json::from_str("{}").unwrap();

        assert_eq!(config.base.adapter, "kafka");
        assert_eq!(config.brokers, vec!["localhost:9092".to_string()]);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn kafka_config_validate_empty_brokers_fails() {
        let mut config = KafkaConfig::default();
        config.brokers.clear();

        let result = config.validate();
        assert!(result.is_err());
    }

    #[test]
    fn kafka_config_rejects_unsupported_exactly_once() {
        let mut config = KafkaConfig::default();
        config.base.delivery_guarantee = DeliveryGuarantee::ExactlyOnce;

        assert!(config.validate().is_err());
    }

    #[test]
    fn security_protocol_display_and_parse_roundtrip() {
        let variants = [
            (SecurityProtocol::Plaintext, "plaintext"),
            (SecurityProtocol::Ssl, "ssl"),
            (SecurityProtocol::SaslPlaintext, "sasl_plaintext"),
            (SecurityProtocol::SaslSsl, "sasl_ssl"),
        ];

        for (variant, expected_str) in &variants {
            let display = format!("{variant}");
            assert_eq!(&display, expected_str);

            let parsed = SecurityProtocol::from_str(&display).unwrap();
            assert_eq!(&parsed, variant);
        }

        let result = SecurityProtocol::from_str("invalid_protocol");
        assert!(result.is_err());
    }

    #[test]
    fn register_adds_kafka_factories_without_creating_clients() {
        let mut registry = MessagingRegistry::<Vec<u8>>::new();
        register(&mut registry, KafkaConfig::default()).unwrap();
        assert_eq!(registry.adapters(), vec!["kafka"]);
    }

    #[test]
    fn kafka_config_rejects_plaintext_without_dev_opt_in_and_bad_names() {
        let mut config = KafkaConfig {
            security_protocol: SecurityProtocol::Plaintext,
            ..KafkaConfig::default()
        };
        assert!(config.validate().is_err());
        config.allow_insecure_dev = true;
        assert!(config.validate().is_ok());

        config = KafkaConfig::default();
        config.base.topics = vec!["bad topic".to_string()];
        assert!(config.validate().is_err());

        config = KafkaConfig {
            brokers: vec!["kafka://user:secret@example.test:9092".to_string()],
            ..KafkaConfig::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn kafka_config_debug_redacts_credentials() {
        let mut config = KafkaConfig {
            sasl_username: Some("alice".to_string()),
            sasl_password: Some("secret".to_string()),
            ..KafkaConfig::default()
        };
        config.brokers = vec!["kafka://broker-user:broker-pass@example.test:9092".to_string()];

        let debug = format!("{config:?}");

        assert!(debug.contains("<redacted>"));
        assert!(!debug.contains("alice"));
        assert!(!debug.contains("secret"));
        assert!(!debug.contains("broker-user"));
        assert!(!debug.contains("broker-pass"));
        assert!(debug.contains("example.test:9092"));
    }
}
