use std::io;

use rskit_messaging::{CommitStrategy, DeliveryGuarantee};

use super::*;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use rskit_errors::{AppError, ErrorCode};
use rskit_messaging::{
    BrokerConfigExt, Event, EventConsumer, EventProducer, Message, MessageConsumer,
    MessageProducer, MessagingFactory, MessagingRegistry,
};
use rskit_stream::SpawnedTask;
use tokio::sync::mpsc;

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
async fn shutdown_consumer_tasks_handles_empty_and_already_finished_tasks() {
    shutdown_consumer_tasks(Vec::new());

    let task = SpawnedTask::spawn(|_cancel| async {});
    tokio::task::yield_now().await;

    shutdown_consumer_tasks(vec![task]);
}

#[tokio::test(start_paused = true)]
async fn shutdown_consumer_tasks_aborts_tasks_that_ignore_cancellation() {
    let task = SpawnedTask::spawn(|_cancel| futures_util::future::pending::<()>());

    shutdown_consumer_tasks(vec![task]);
    tokio::time::advance(Duration::from_millis(101)).await;
    tokio::task::yield_now().await;
}

#[test]
fn shutdown_consumer_tasks_aborts_without_runtime_handle() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .unwrap();
    let task = {
        let _guard = runtime.enter();
        SpawnedTask::spawn(|_cancel| futures_util::future::pending::<()>())
    };
    drop(runtime);

    shutdown_consumer_tasks(vec![task]);
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
    task.join().await;
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
    task.join().await;

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
    task.join().await;

    let (sender, _receiver) = mpsc::channel(1);
    let stream = futures_util::stream::pending::<Result<(String, Vec<u8>), std::io::Error>>();
    let task = spawn_forwarding_task(
        "events".to_string(),
        stream,
        sender,
        active_tasks,
        task_finished.clone(),
    );
    task.cancel();
    task.join().await;
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
    let stream = futures_util::stream::iter(vec![Ok::<_, std::io::Error>((
        "events".to_string(),
        b"payload".to_vec(),
    ))]);
    let task = spawn_forwarding_task(
        "events".to_string(),
        stream,
        sender,
        active_tasks.clone(),
        task_finished,
    );

    tokio::task::yield_now().await;
    task.cancel();
    task.join().await;

    assert_eq!(active_tasks.load(Ordering::SeqCst), 0);
    assert_eq!(receiver.recv().await.unwrap().topic, "filled");
    assert!(receiver.try_recv().is_err());
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

    let result = consumer.recv(std::time::Duration::from_secs(1)).await;

    assert!(result.is_err());
}

#[tokio::test(start_paused = true)]
async fn recv_times_out_when_no_messages_arrive() {
    let consumer = Arc::new(RabbitMqConsumer::new(Config::default()).unwrap());
    let waiter = consumer.clone();
    let handle = tokio::spawn(async move { waiter.recv(Duration::from_secs(5)).await });

    tokio::time::advance(Duration::from_secs(6)).await;

    let err = handle.await.unwrap().unwrap_err();
    assert_eq!(err.code(), ErrorCode::Timeout);
}

#[test]
fn rabbitmq_config_rejects_at_least_once_delivery() {
    let mut config = Config::default();
    config.base.delivery_guarantee = DeliveryGuarantee::AtLeastOnce;
    let err = config.validate().unwrap_err();
    assert_eq!(err.code(), ErrorCode::InvalidInput);
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

    let err = consumer
        .recv_event(Duration::from_secs(1))
        .await
        .unwrap_err();
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
