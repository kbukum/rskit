use super::*;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use rskit_errors::ErrorCode;
use rskit_messaging::{
    BrokerConfigExt, DeliveryGuarantee, Event, EventConsumer, EventProducer, Message,
    MessageConsumer, MessageProducer, MessagingFactory, MessagingRegistry,
};
use rskit_stream::SpawnedTask;
use tokio::sync::mpsc;

#[test]
fn register_adds_nats_factories_without_connecting() {
    let mut registry = MessagingRegistry::<Vec<u8>>::new();
    register(&mut registry, Config::default()).unwrap();
    assert_eq!(registry.adapters(), vec!["nats"]);
}

#[test]
fn register_skips_disabled_nats_backend() {
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
fn register_rejects_duplicate_nats_backend() {
    let mut registry = MessagingRegistry::<Vec<u8>>::new();
    register(&mut registry, Config::default()).unwrap();

    let err = register(&mut registry, Config::default()).unwrap_err();

    assert_eq!(err.code(), ErrorCode::AlreadyExists);
}

#[test]
fn nats_config_deserializes_adapter_defaults() {
    let config: Config = serde_json::from_str("{}").unwrap();

    assert_eq!(config.base.adapter, "nats");
    assert_eq!(config.servers, vec!["tls://127.0.0.1:4222".to_string()]);
    assert!(config.validate().is_ok());
}

#[test]
fn nats_config_debug_redacts_url_credentials() {
    let config = Config {
        servers: vec!["nats://token:secret@example.test:4222".to_string()],
        subscription_buffer: 1,
        ..Config::default()
    };

    let debug = format!("{config:?}");

    assert!(debug.contains("<redacted>"));
    assert!(debug.contains("example.test:4222"));
    assert!(!debug.contains("token:secret"));
    assert!(!debug.contains("secret"));
}

#[test]
fn nats_config_validate_rejects_unsupported_semantics_and_bad_auth() {
    let mut config = Config::default();
    config.base.delivery_guarantee = DeliveryGuarantee::ExactlyOnce;
    assert!(config.validate().is_err());

    config = Config::default();
    config.username = Some("user".to_string());
    assert!(config.validate().is_err());

    config = Config::default();
    config.token = Some("token".to_string());
    config.password = Some("password".to_string());
    config.username = Some("user".to_string());
    assert!(config.validate().is_err());
}

#[test]
fn nats_config_rejects_all_validation_edge_cases() {
    let mut config = Config::default();
    config.base.adapter = "other".to_string();
    assert!(config.validate().is_err());

    config = Config::default();
    config.base.commit_strategy = rskit_messaging::CommitStrategy::Manual;
    assert!(config.validate().is_err());

    config = Config::default();
    config.base.retries = 1;
    assert!(config.validate().is_err());

    config = Config::default();
    config.base.dlq.enabled = true;
    assert!(config.validate().is_err());

    config = Config::default();
    config.servers.clear();
    assert!(config.validate().is_err());

    config = Config::default();
    config.servers = vec![String::new()];
    assert!(config.validate().is_err());

    config = Config::default();
    config.servers = vec!["tls://nats.example.test:4222?token=secret".to_string()];
    assert!(config.validate().is_err());

    config = Config::default();
    config.subject_prefix = "bad prefix.".to_string();
    assert!(config.validate().is_err());

    config = Config::default();
    config.base.topics = vec!["bad subject".to_string()];
    assert!(config.validate().is_err());

    config = Config::default();
    config.connection_timeout = 0;
    assert!(config.validate().is_err());

    config = Config::default();
    config.reconnect_delay = 0;
    assert!(config.validate().is_err());

    config = Config::default();
    config.subscription_buffer = 0;
    assert!(config.validate().is_err());
}

#[test]
fn nats_config_rejects_plaintext_without_dev_opt_in_and_bad_names() {
    let mut config = Config {
        servers: vec!["nats://127.0.0.1:4222".to_string()],
        ..Config::default()
    };
    assert!(config.validate().is_err());
    config.allow_insecure_dev = true;
    assert!(config.validate().is_ok());

    config = Config {
        servers: vec!["tls://token:secret@example.test:4222".to_string()],
        ..Config::default()
    };
    assert!(config.validate().is_err());

    config = Config::default();
    config.base.consumer_group = Some("bad group".to_string());
    assert!(config.validate().is_err());
}

#[test]
fn nats_subject_prefix_is_applied_consistently() {
    let config = Config {
        subject_prefix: "svc.".to_string(),
        ..Config::default()
    };

    assert_eq!(subject_for(&config, "events").unwrap(), "svc.events");
}

#[test]
fn nats_connect_options_accept_supported_auth_modes() {
    let token_config = Config {
        token: Some("token".to_string()),
        ..Config::default()
    };
    let user_config = Config {
        username: Some("user".to_string()),
        password: Some("password".to_string()),
        max_reconnects: Some(2),
        ..Config::default()
    };

    let _token_options = connect_options(&token_config);
    let _user_options = connect_options(&user_config);
}

#[tokio::test]
async fn nats_producer_rejects_invalid_subject_before_connecting() {
    let producer = NatsProducer::new(Config::default()).unwrap();

    let err = producer
        .send(Message::new("bad subject", Vec::from("payload")))
        .await
        .unwrap_err();

    assert_eq!(err.code(), ErrorCode::InvalidInput);
}

#[tokio::test]
async fn nats_empty_batches_and_close_do_not_connect() {
    let producer = NatsProducer::new(Config::default()).unwrap();

    producer.send_batch(Vec::new()).await.unwrap();
    producer.publish_batch("events", Vec::new()).await.unwrap();
    producer.close().await.unwrap();
}

#[tokio::test]
async fn producer_and_consumer_constructors_validate_without_connecting() {
    let valid = Config::default();

    NatsProducer::new(valid.clone()).unwrap();
    NatsConsumer::new(valid).unwrap();

    let invalid = Config {
        subscription_buffer: 0,
        ..Config::default()
    };
    assert!(NatsProducer::new(invalid.clone()).is_err());
    assert!(NatsConsumer::new(invalid).is_err());
}

#[tokio::test]
async fn consumer_empty_subscribe_and_close_do_not_connect() {
    let consumer = NatsConsumer::new(Config::default()).unwrap();

    MessageConsumer::subscribe(&consumer, &[]).await.unwrap();
    consumer.close().await.unwrap();

    assert!(!consumer.subscribed.load(Ordering::SeqCst));
}

#[tokio::test]
async fn shutdown_consumer_tasks_requests_cancellation() {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    let task = SpawnedTask::spawn(move |cancel| async move {
        cancel.cancelled().await;
        let _ = sender.send(());
    });

    shutdown_consumer_tasks(vec![task]);

    tokio::time::timeout(Duration::from_secs(1), receiver)
        .await
        .unwrap()
        .unwrap();
}

#[tokio::test]
async fn shutdown_consumer_tasks_aborts_tasks_that_ignore_cancellation() {
    let task = SpawnedTask::spawn(|_cancel| async move {
        futures_util::future::pending::<()>().await;
    });

    shutdown_consumer_tasks(vec![task]);

    tokio::time::sleep(Duration::from_millis(150)).await;
}

#[test]
fn shutdown_consumer_tasks_aborts_without_runtime_handle() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .unwrap();
    let task = {
        let _guard = runtime.enter();
        SpawnedTask::spawn(|_cancel| async move {
            futures_util::future::pending::<()>().await;
        })
    };
    drop(runtime);

    shutdown_consumer_tasks(vec![task]);
}

#[tokio::test]
async fn forwarding_task_stops_when_cancelled_while_send_is_backpressured() {
    let (sender, mut receiver) = mpsc::channel(1);
    sender
        .send(Message::new("filled", b"first".to_vec()))
        .await
        .unwrap();
    let active_tasks = Arc::new(AtomicUsize::new(0));
    let task_finished = Arc::new(tokio::sync::Notify::new());
    let deliveries = futures_util::stream::iter([("blocked".to_string(), b"second".to_vec())]);

    let task = spawn_forwarding_task(
        "events".to_string(),
        deliveries,
        sender,
        active_tasks.clone(),
        task_finished.clone(),
    );

    // Wait until the spawned task has started (it increments the counter
    // before blocking on the backpressured send) without a wall-clock sleep.
    while active_tasks.load(Ordering::SeqCst) == 0 {
        tokio::task::yield_now().await;
    }
    assert_eq!(active_tasks.load(Ordering::SeqCst), 1);

    shutdown_consumer_tasks(vec![task]);

    tokio::time::timeout(Duration::from_secs(1), task_finished.notified())
        .await
        .unwrap();
    assert_eq!(active_tasks.load(Ordering::SeqCst), 0);
    assert_eq!(receiver.recv().await.unwrap().topic, "filled");
    assert!(receiver.try_recv().is_err());
}

#[test]
fn nats_subject_prefix_validates_combined_subject() {
    let config = Config {
        subject_prefix: "svc.".to_string(),
        base: rskit_messaging::BrokerConfig {
            topics: vec!["bad subject".to_string()],
            ..Config::default().base
        },
        ..Config::default()
    };
    assert!(config.validate().is_err());

    let config = Config {
        subject_prefix: "svc..".to_string(),
        base: rskit_messaging::BrokerConfig {
            topics: vec!["events".to_string()],
            ..Config::default().base
        },
        ..Config::default()
    };
    assert!(config.validate().is_err());
}

#[tokio::test]
async fn recv_returns_when_subscribed_tasks_have_closed() {
    let consumer = NatsConsumer::new(Config::default()).unwrap();
    consumer.subscribed.store(true, Ordering::SeqCst);

    let result = consumer.recv(std::time::Duration::from_secs(1)).await;

    assert!(result.is_err());
}

#[test]
fn nats_factory_creates_lazy_producer_and_consumer() {
    let factory = NatsFactory {
        config: Config::default(),
    };
    let broker = rskit_messaging::BrokerConfig::default();

    factory.create_producer(&broker).unwrap();
    factory.create_consumer(&broker).unwrap();
}

#[tokio::test]
async fn event_wrappers_validate_and_decode_through_message_paths() {
    let producer = NatsProducer::new(Config::default()).unwrap();
    let event = Event::new("example.created", "test");

    let err = producer.publish("bad subject", event).await.unwrap_err();
    assert_eq!(err.code(), ErrorCode::InvalidInput);

    let consumer = NatsConsumer::new(Config::default()).unwrap();
    EventConsumer::subscribe(&consumer, &[]).await.unwrap();
    consumer
        .sender
        .send(Message::new("events", b"not-json".to_vec()))
        .await
        .unwrap();

    let err = consumer
        .recv_event(Duration::from_secs(1))
        .await
        .unwrap_err();
    assert_eq!(err.code(), ErrorCode::Internal);
}

#[tokio::test]
async fn producer_and_consumer_report_fast_connection_failures() {
    let config = Config {
        servers: vec!["nats://127.0.0.1:1".to_string()],
        allow_insecure_dev: true,
        connection_timeout: 1,
        max_reconnects: Some(0),
        ..Config::default()
    };

    let producer = NatsProducer::new(config.clone()).unwrap();
    let err = producer
        .send(Message::new("events", Vec::from("payload")))
        .await
        .unwrap_err();
    assert_eq!(err.code(), ErrorCode::ExternalService);

    let consumer = NatsConsumer::new(config).unwrap();
    let err = MessageConsumer::subscribe(&consumer, &["events"])
        .await
        .unwrap_err();
    assert_eq!(err.code(), ErrorCode::ExternalService);
}

#[tokio::test]
async fn producer_flush_reports_fast_connection_failures() {
    let producer = NatsProducer::new(Config {
        servers: vec!["nats://127.0.0.1:1".to_string()],
        allow_insecure_dev: true,
        connection_timeout: 1,
        max_reconnects: Some(0),
        ..Config::default()
    })
    .unwrap();

    let err = producer.flush(Duration::ZERO).await.unwrap_err();

    assert_eq!(err.code(), ErrorCode::ExternalService);
}

#[test]
fn nats_error_helpers_preserve_external_service_context() {
    let errors = [
        (nats_connect_error("connect failed"), "NATS connect failed"),
        (nats_publish_error("publish failed"), "NATS publish failed"),
        (nats_flush_error("flush failed"), "NATS flush failed"),
        (
            nats_close_flush_error("close flush failed"),
            "NATS flush before close failed",
        ),
        (
            nats_subscribe_error("subscribe failed"),
            "NATS subscribe failed",
        ),
    ];

    for (error, message) in errors {
        assert_eq!(error.code(), ErrorCode::ExternalService);
        assert!(error.to_string().contains(message));
    }
}
