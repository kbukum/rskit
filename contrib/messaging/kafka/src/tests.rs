use std::str::FromStr;

use super::*;
use std::time::Duration;

use crate::config::SecurityProtocol;
use rdkafka::error::KafkaError;
use rdkafka::types::RDKafkaErrorCode;
use rskit_errors::ErrorCode;
use rskit_messaging::{
    BrokerConfigExt, CommitStrategy, DeliveryGuarantee, Event, EventConsumer, EventProducer,
    Message, MessageProducer, MessagingFactory, MessagingRegistry,
};

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
