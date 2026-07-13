//! Kafka adapter for `rskit-messaging`.
//!
//! Registration is explicit and side-effect free: call [`register`] from
//! application composition code to add Kafka factories to a [`MessagingRegistry`].

#![warn(missing_docs)]

use std::future::Future;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use rdkafka::Message as RdMessage;
use rdkafka::config::ClientConfig;
use rdkafka::consumer::{Consumer, StreamConsumer};
use rdkafka::error::KafkaError;
use rdkafka::producer::{FutureProducer, FutureRecord, Producer};
use rskit_errors::{AppError, AppResult, ErrorCode};
use rskit_messaging::{
    BrokerConfigExt, CommitStrategy, DeliveryGuarantee, Event, EventConsumer, EventProducer,
    Message, MessageConsumer, MessageProducer, MessagingFactory, MessagingRegistry,
};
use tokio_stream::StreamExt;
use tracing::debug;

mod config;

use crate::config::validate_topic;
pub use config::{Compression, KafkaConfig as Config, OffsetReset, SecurityProtocol};

/// Kafka-backed message producer wrapping an `rdkafka` `FutureProducer`.
pub(crate) struct KafkaProducer {
    producer: FutureProducer,
}

impl KafkaProducer {
    /// Create a new `KafkaProducer` from the given configuration.
    pub(crate) fn new(config: &Config) -> AppResult<Self> {
        config.validate()?;
        let producer: FutureProducer = producer_config(config)
            .create()
            .map_err(|error| kafka_producer_creation_error(&error))?;
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
            .map_err(|(error, _)| kafka_send_error(&error))?;

        debug!(topic = %msg.topic, "message sent to Kafka");
        Ok(())
    }

    async fn send_batch(&self, msgs: Vec<Message<Vec<u8>>>) -> AppResult<()> {
        for msg in &msgs {
            validate_topic("Kafka topic", &msg.topic)?;
        }
        let mut deliveries = Vec::with_capacity(msgs.len());
        for msg in &msgs {
            let mut record = FutureRecord::to(&msg.topic).payload(&msg.payload);
            if let Some(key) = msg.key.as_deref() {
                record = record.key(key);
            }
            let delivery = self
                .producer
                .send_result(record)
                .map_err(|(error, _)| kafka_send_error(&error))?;
            deliveries.push(delivery);
        }
        for delivery in deliveries {
            delivery
                .await
                .map_err(|error| {
                    AppError::new(
                        ErrorCode::ExternalService,
                        "Kafka delivery future was cancelled",
                    )
                    .with_cause(error)
                })?
                .map_err(|(error, _)| kafka_send_error(&error))?;
        }
        Ok(())
    }

    async fn flush(&self, timeout: Duration) -> AppResult<()> {
        self.producer
            .flush(timeout)
            .map_err(|error| kafka_flush_error(&error))
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
        validate_topic("Kafka topic", topic)?;
        let mut msgs = Vec::with_capacity(events.len());
        for event in events {
            let payload = event.to_json()?;
            msgs.push(Message::new(topic, payload).with_key(&event.id));
        }
        self.send_batch(msgs).await
    }
}

/// Kafka-backed message consumer wrapping an `rdkafka` `StreamConsumer`.
pub(crate) struct KafkaConsumer {
    consumer: StreamConsumer,
}

impl KafkaConsumer {
    /// Create a new `KafkaConsumer` from the given configuration.
    pub(crate) fn new(config: &Config) -> AppResult<Self> {
        config.validate()?;
        let consumer: StreamConsumer = consumer_config(config)
            .create()
            .map_err(|error| kafka_consumer_creation_error(&error))?;
        Ok(Self { consumer })
    }
}

#[async_trait]
impl MessageConsumer<Vec<u8>> for KafkaConsumer {
    async fn subscribe(&self, topics: &[&str]) -> AppResult<()> {
        for topic in topics {
            validate_topic("Kafka topic", topic)?;
        }
        self.consumer
            .subscribe(topics)
            .map_err(|error| kafka_subscribe_error(&error))
    }

    async fn recv(&self, timeout: Duration) -> AppResult<Message<Vec<u8>>> {
        if timeout.is_zero() {
            return Err(AppError::new(
                ErrorCode::InvalidInput,
                "Kafka receive timeout must be greater than zero",
            ));
        }
        let mut stream = self.consumer.stream();
        let borrowed = recv_next_with_timeout(stream.next(), timeout)
            .await?
            .ok_or_else(kafka_stream_ended_error)?;

        let borrowed = borrowed.map_err(|error| kafka_receive_error(&error))?;

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

fn kafka_producer_creation_error(error: &KafkaError) -> AppError {
    AppError::new(
        ErrorCode::Internal,
        format!("failed to create Kafka producer: {error}"),
    )
}

fn kafka_send_error(error: &KafkaError) -> AppError {
    let code = if matches!(
        error,
        KafkaError::MessageProduction(rdkafka::types::RDKafkaErrorCode::QueueFull)
    ) {
        ErrorCode::RateLimited
    } else {
        ErrorCode::ExternalService
    };
    AppError::new(code, format!("Kafka send failed: {error}"))
}

fn kafka_flush_error(error: &KafkaError) -> AppError {
    AppError::new(
        ErrorCode::ExternalService,
        format!("Kafka flush failed: {error}"),
    )
}

fn kafka_consumer_creation_error(error: &KafkaError) -> AppError {
    AppError::new(
        ErrorCode::Internal,
        format!("failed to create Kafka consumer: {error}"),
    )
}

fn kafka_subscribe_error(error: &KafkaError) -> AppError {
    AppError::new(
        ErrorCode::ExternalService,
        format!("Kafka subscribe failed: {error}"),
    )
}

fn kafka_stream_ended_error() -> AppError {
    AppError::new(
        ErrorCode::ExternalService,
        "Kafka stream ended unexpectedly",
    )
}

async fn recv_next_with_timeout<F, T>(future: F, timeout: Duration) -> AppResult<T>
where
    F: Future<Output = T>,
{
    tokio::time::timeout(timeout, future)
        .await
        .map_err(|error| AppError::timeout("Kafka receive").with_cause(error))
}

fn kafka_receive_error(error: &KafkaError) -> AppError {
    AppError::new(
        ErrorCode::ExternalService,
        format!("Kafka receive error: {error}"),
    )
}

#[async_trait]
impl EventConsumer for KafkaConsumer {
    async fn subscribe(&self, topics: &[&str]) -> AppResult<()> {
        MessageConsumer::subscribe(self, topics).await
    }

    async fn recv_event(&self, timeout: Duration) -> AppResult<Event> {
        let msg = MessageConsumer::recv(self, timeout).await?;
        Event::from_json(&msg.payload)
    }
}

/// Register Kafka producer and consumer factories for `Vec<u8>` payloads.
pub fn register(registry: &mut MessagingRegistry<Vec<u8>>, config: Config) -> AppResult<()> {
    config.validate()?;
    if !config.base.enabled {
        return Ok(());
    }
    let adapter = config.base.adapter.clone();
    registry.register_backend(adapter, Arc::new(KafkaFactory { config }))
}

struct KafkaFactory {
    config: Config,
}

impl MessagingFactory<Vec<u8>> for KafkaFactory {
    fn create_producer(
        &self,
        _config: &rskit_messaging::BrokerConfig,
    ) -> AppResult<Arc<dyn MessageProducer<Vec<u8>>>> {
        Ok(Arc::new(KafkaProducer::new(&self.config)?))
    }

    fn create_consumer(
        &self,
        _config: &rskit_messaging::BrokerConfig,
    ) -> AppResult<Arc<dyn MessageConsumer<Vec<u8>>>> {
        Ok(Arc::new(KafkaConsumer::new(&self.config)?))
    }
}

fn base_client_config(config: &Config) -> ClientConfig {
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

fn producer_config(config: &Config) -> ClientConfig {
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
    cfg.set(
        "queue.buffering.max.messages",
        config.queue_capacity.to_string(),
    );
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

fn consumer_config(config: &Config) -> ClientConfig {
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
    use crate::config::SecurityProtocol;
    use rdkafka::types::RDKafkaErrorCode;

    #[test]
    fn kafka_config_default_values() {
        let config = Config::default();

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
        let config: Config = serde_json::from_str("{}").unwrap();

        assert_eq!(config.base.adapter, "kafka");
        assert_eq!(config.brokers, vec!["localhost:9092".to_string()]);
        assert!(config.validate().is_ok());
    }

    #[test]
    fn kafka_config_validate_empty_brokers_fails() {
        let mut config = Config::default();
        config.brokers.clear();

        let result = config.validate();
        assert!(result.is_err());
    }

    #[test]
    fn kafka_config_rejects_unsupported_exactly_once() {
        let mut config = Config::default();
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
        register(&mut registry, Config::default()).unwrap();
        assert_eq!(registry.adapters(), vec!["kafka"]);
    }

    #[test]
    fn register_skips_disabled_kafka_backend() {
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
    fn register_rejects_duplicate_kafka_backend() {
        let mut registry = MessagingRegistry::<Vec<u8>>::new();
        register(&mut registry, Config::default()).unwrap();

        let err = register(&mut registry, Config::default()).unwrap_err();

        assert_eq!(err.code(), ErrorCode::AlreadyExists);
    }

    #[test]
    fn kafka_config_rejects_plaintext_without_dev_opt_in_and_bad_names() {
        let mut config = Config {
            security_protocol: SecurityProtocol::Plaintext,
            ..Config::default()
        };
        assert!(config.validate().is_err());
        config.allow_insecure_dev = true;
        assert!(config.validate().is_ok());

        config = Config::default();
        config.base.topics = vec!["bad topic".to_string()];
        assert!(config.validate().is_err());

        config = Config {
            brokers: vec!["kafka://user:secret@example.test:9092".to_string()],
            ..Config::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn kafka_config_rejects_all_validation_edge_cases() {
        let mut config = Config::default();
        config.base.adapter = "other".to_string();
        assert!(config.validate().is_err());

        config = Config::default();
        config.base.dlq.enabled = true;
        assert!(config.validate().is_err());

        config = Config::default();
        config.base.commit_strategy = CommitStrategy::Manual;
        assert!(config.validate().is_err());

        config = Config::default();
        config.brokers = vec![String::new()];
        assert!(config.validate().is_err());

        config = Config::default();
        config.brokers = vec!["broker.example.test:9092?secret=true".to_string()];
        assert!(config.validate().is_err());

        config = Config::default();
        config.base.subscriptions = vec!["bad topic".to_string()];
        assert!(config.validate().is_err());

        config = Config::default();
        config.base.consumer_group = Some("bad group".to_string());
        assert!(config.validate().is_err());

        config = Config::default();
        config.batch_size = 0;
        assert!(config.validate().is_err());

        config = Config::default();
        config.session_timeout = Duration::ZERO;
        assert!(config.validate().is_err());

        config = Config {
            security_protocol: SecurityProtocol::SaslSsl,
            sasl_username: Some("user".to_string()),
            sasl_password: None,
            ..Config::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn kafka_client_configs_map_shared_and_adapter_settings() {
        let mut config = Config {
            brokers: vec!["kafka-a:9092".to_string(), "kafka-b:9092".to_string()],
            compression: Compression::Zstd,
            auto_offset_reset: OffsetReset::Earliest,
            security_protocol: SecurityProtocol::SaslSsl,
            sasl_mechanism: Some("PLAIN".to_string()),
            sasl_username: Some("alice".to_string()),
            sasl_password: Some("secret".to_string()),
            ..Config::default()
        };
        config.base.request_timeout = Some(1234);
        config.base.retry_backoff = 25;
        config.base.max_in_flight = 7;
        config.base.consumer_group = Some("workers".to_string());

        let producer = producer_config(&config);
        assert_eq!(
            producer.get("bootstrap.servers"),
            Some("kafka-a:9092,kafka-b:9092")
        );
        assert_eq!(producer.get("security.protocol"), Some("sasl_ssl"));
        assert_eq!(producer.get("sasl.mechanism"), Some("PLAIN"));
        assert_eq!(producer.get("sasl.username"), Some("alice"));
        assert_eq!(producer.get("sasl.password"), Some("secret"));
        assert_eq!(producer.get("request.timeout.ms"), Some("1234"));
        assert_eq!(producer.get("retry.backoff.ms"), Some("25"));
        assert_eq!(producer.get("compression.type"), Some("zstd"));
        assert_eq!(producer.get("batch.size"), Some("1000"));
        assert_eq!(producer.get("linger.ms"), Some("5"));
        assert_eq!(producer.get("message.send.max.retries"), Some("3"));
        assert_eq!(
            producer.get("max.in.flight.requests.per.connection"),
            Some("7")
        );

        let consumer = consumer_config(&config);
        assert_eq!(consumer.get("group.id"), Some("workers"));
        assert_eq!(consumer.get("auto.offset.reset"), Some("earliest"));
        assert_eq!(consumer.get("enable.auto.commit"), Some("true"));
        assert_eq!(consumer.get("session.timeout.ms"), Some("30000"));
    }

    #[test]
    fn kafka_producer_config_maps_all_compression_and_ack_variants() {
        let cases = [
            (Compression::None, "none"),
            (Compression::Gzip, "gzip"),
            (Compression::Snappy, "snappy"),
            (Compression::Lz4, "lz4"),
            (Compression::Zstd, "zstd"),
        ];

        for (compression, expected) in cases {
            let config = Config {
                compression,
                ..Config::default()
            };

            assert_eq!(
                producer_config(&config).get("compression.type"),
                Some(expected)
            );
        }

        let config = Config {
            base: rskit_messaging::BrokerConfig {
                delivery_guarantee: DeliveryGuarantee::AtMostOnce,
                ..Config::default().base
            },
            ..Config::default()
        };
        assert_eq!(producer_config(&config).get("acks"), Some("0"));
    }

    #[test]
    fn kafka_consumer_config_omits_group_when_unset_and_maps_manual_commit() {
        let config = Config::default();
        let consumer = consumer_config(&config);

        assert_eq!(consumer.get("group.id"), None);
        assert_eq!(consumer.get("auto.offset.reset"), Some("latest"));
        assert_eq!(consumer.get("enable.auto.commit"), Some("true"));

        let config = Config {
            base: rskit_messaging::BrokerConfig {
                commit_strategy: CommitStrategy::Manual,
                ..Config::default().base
            },
            ..Config::default()
        };
        assert_eq!(
            consumer_config(&config).get("enable.auto.commit"),
            Some("false")
        );
    }

    #[tokio::test]
    async fn kafka_producer_rejects_invalid_topic_before_send() {
        let config = Config {
            security_protocol: SecurityProtocol::Plaintext,
            allow_insecure_dev: true,
            ..Config::default()
        };
        let producer = KafkaProducer::new(&config).unwrap();

        let err = producer
            .send(Message::new("bad topic", Vec::from("payload")))
            .await
            .unwrap_err();

        assert_eq!(err.code(), ErrorCode::InvalidInput);
    }

    #[tokio::test]
    async fn kafka_empty_batches_do_not_touch_broker() {
        let config = Config {
            security_protocol: SecurityProtocol::Plaintext,
            allow_insecure_dev: true,
            ..Config::default()
        };
        let producer = KafkaProducer::new(&config).unwrap();

        producer.send_batch(Vec::new()).await.unwrap();
        producer.publish_batch("events", Vec::new()).await.unwrap();
    }

    #[test]
    fn kafka_config_debug_redacts_credentials() {
        let mut config = Config {
            sasl_username: Some("alice".to_string()),
            sasl_password: Some("secret".to_string()),
            ..Config::default()
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

    #[tokio::test]
    async fn kafka_factory_creates_producer_and_consumer_with_plaintext_dev_config() {
        let factory = KafkaFactory {
            config: Config {
                security_protocol: SecurityProtocol::Plaintext,
                allow_insecure_dev: true,
                ..Config::default()
            },
        };
        let broker = rskit_messaging::BrokerConfig::default();

        factory.create_producer(&broker).unwrap();
        factory.create_consumer(&broker).unwrap();
    }

    #[tokio::test]
    async fn event_wrappers_validate_topics_before_broker_interaction() {
        let config = Config {
            security_protocol: SecurityProtocol::Plaintext,
            allow_insecure_dev: true,
            ..Config::default()
        };
        let producer = KafkaProducer::new(&config).unwrap();
        let event = Event::new("example.created", "test");

        let err = producer.publish("bad topic", event).await.unwrap_err();
        assert_eq!(err.code(), ErrorCode::InvalidInput);

        let consumer = KafkaConsumer::new(&config).unwrap();
        let err = EventConsumer::subscribe(&consumer, &["bad topic"])
            .await
            .unwrap_err();
        assert_eq!(err.code(), ErrorCode::InvalidInput);
    }

    #[tokio::test]
    async fn kafka_flush_with_zero_timeout_completes_without_broker() {
        let config = Config {
            security_protocol: SecurityProtocol::Plaintext,
            allow_insecure_dev: true,
            ..Config::default()
        };
        let producer = KafkaProducer::new(&config).unwrap();

        producer.flush(Duration::ZERO).await.unwrap();
    }

    #[tokio::test]
    async fn kafka_recv_returns_timeout_when_no_message_arrives() {
        let err = recv_next_with_timeout(std::future::pending::<()>(), Duration::from_millis(1))
            .await
            .unwrap_err();

        assert_eq!(err.code(), ErrorCode::Timeout);
    }

    #[test]
    fn kafka_bounded_queue_config_and_backpressure_error_are_typed() {
        let config = Config {
            queue_capacity: 17,
            ..Config::default()
        };

        assert_eq!(
            producer_config(&config).get("queue.buffering.max.messages"),
            Some("17")
        );
        assert_eq!(
            kafka_send_error(&KafkaError::MessageProduction(RDKafkaErrorCode::QueueFull)).code(),
            ErrorCode::RateLimited
        );
    }

    #[tokio::test]
    async fn kafka_bounded_queue_returns_backpressure_when_full() {
        let config = Config {
            security_protocol: SecurityProtocol::Plaintext,
            allow_insecure_dev: true,
            queue_capacity: 1,
            ..Config::default()
        };
        let producer = KafkaProducer::new(&config).unwrap();

        let err = producer
            .send_batch(vec![
                Message::new("events", b"first".to_vec()),
                Message::new("events", b"second".to_vec()),
            ])
            .await
            .unwrap_err();

        assert_eq!(err.code(), ErrorCode::RateLimited);
    }

    #[test]
    fn kafka_error_helpers_preserve_error_codes_and_context() {
        let producer_error = KafkaError::ClientCreation("producer".to_string());
        let consumer_error = KafkaError::ClientCreation("consumer".to_string());
        let creation = kafka_producer_creation_error(&producer_error);
        let consumer = kafka_consumer_creation_error(&consumer_error);
        let external = [
            kafka_send_error(&KafkaError::MessageProduction(
                RDKafkaErrorCode::MessageTimedOut,
            )),
            kafka_flush_error(&KafkaError::Flush(RDKafkaErrorCode::QueueFull)),
            kafka_subscribe_error(&KafkaError::Subscription("bad topic".to_string())),
            kafka_stream_ended_error(),
            kafka_receive_error(&KafkaError::MessageConsumption(RDKafkaErrorCode::QueueFull)),
        ];

        assert_eq!(creation.code(), ErrorCode::Internal);
        assert!(
            creation
                .to_string()
                .contains("failed to create Kafka producer")
        );
        assert_eq!(consumer.code(), ErrorCode::Internal);
        assert!(
            consumer
                .to_string()
                .contains("failed to create Kafka consumer")
        );
        for error in external {
            assert_eq!(error.code(), ErrorCode::ExternalService);
            assert!(error.to_string().contains("Kafka"));
        }
    }
}
