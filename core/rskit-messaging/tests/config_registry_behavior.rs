//! Behavioral tests for messaging configuration, registry, and composition APIs.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use async_trait::async_trait;
use rskit_errors::{AppError, AppResult, ErrorCode};
use rskit_messaging::{
    BrokerConfig, BrokerConfigOverrides, CommitStrategy, DeliveryGuarantee, DlqPolicy, FnHandler,
    HandlerMiddleware, InMemoryBroker, ManagedConsumerBuilder, ManagedProducerBuilder, Message,
    MessageConsumer, MessageHandler, MessageProducer, MessagingFactory, MessagingRegistry,
    StackBuilder, middleware_fn,
};
use rskit_resilience::{LinearBackoff, RetryPolicy};

#[test]
fn broker_config_overrides_replace_every_shared_field() {
    let mut config = BrokerConfig::default();
    BrokerConfigOverrides {
        adapter: Some("custom".to_owned()),
        name: Some("primary".to_owned()),
        enabled: Some(false),
        retries: Some(7),
        retry_backoff: Some(250),
        request_timeout: Some(Some(1_500)),
        delivery_guarantee: Some(DeliveryGuarantee::ExactlyOnce),
        commit_strategy: Some(CommitStrategy::Manual),
        dlq: Some(DlqPolicy {
            enabled: false,
            suffix: ".dead".to_owned(),
        }),
        max_in_flight: Some(32),
        consumer_group: Some(Some("orders-workers".to_owned())),
        topics: Some(vec!["orders.created".to_owned()]),
        subscriptions: Some(vec!["orders.*".to_owned()]),
    }
    .apply_to(&mut config);

    assert_eq!(config.adapter, "custom");
    assert_eq!(config.name, "primary");
    assert!(!config.enabled);
    assert_eq!(config.retries, 7);
    assert_eq!(config.retry_backoff_duration(), Duration::from_millis(250));
    assert_eq!(
        config.request_timeout_duration(),
        Some(Duration::from_millis(1_500))
    );
    assert_eq!(config.delivery_guarantee, DeliveryGuarantee::ExactlyOnce);
    assert_eq!(config.commit_strategy, CommitStrategy::Manual);
    assert!(!config.dlq.enabled);
    assert_eq!(config.max_in_flight, 32);
    assert_eq!(config.consumer_group.as_deref(), Some("orders-workers"));
    assert_eq!(config.topics, ["orders.created"]);
    assert_eq!(config.subscriptions, ["orders.*"]);
}

#[test]
fn broker_config_validation_rejects_topic_and_name_edge_cases() {
    let invalid_cases = [
        BrokerConfig {
            topics: vec![" ".to_owned()],
            ..Default::default()
        },
        BrokerConfig {
            subscriptions: vec!["bad subscription".to_owned()],
            ..Default::default()
        },
        BrokerConfig {
            consumer_group: Some("bad group".to_owned()),
            ..Default::default()
        },
        BrokerConfig {
            adapter: "bad/adapter".to_owned(),
            ..Default::default()
        },
        BrokerConfig {
            name: "x".repeat(129),
            ..Default::default()
        },
        BrokerConfig {
            dlq: DlqPolicy {
                enabled: true,
                suffix: "bad/suffix".to_owned(),
            },
            ..Default::default()
        },
    ];

    for config in invalid_cases {
        let error = config.validate().expect_err("config should be rejected");
        assert_eq!(error.code(), ErrorCode::InvalidInput);
    }
}

#[tokio::test]
async fn default_trait_methods_report_capabilities_and_close_cleanly() {
    let producer = NoopProducer;
    let consumer = NoopConsumer;

    producer.close().await.unwrap();
    consumer.close().await.unwrap();

    let pause = consumer.pause().await.unwrap_err();
    let resume = consumer.resume().await.unwrap_err();

    assert_eq!(pause.code(), ErrorCode::InvalidInput);
    assert!(pause.message().contains("pause"));
    assert_eq!(resume.code(), ErrorCode::InvalidInput);
    assert!(resume.message().contains("resume"));
}

#[tokio::test]
async fn messaging_registry_builds_backend_and_individual_consumer() {
    let producer_calls = Arc::new(AtomicUsize::new(0));
    let consumer_calls = Arc::new(AtomicUsize::new(0));
    let mut registry = MessagingRegistry::<String>::new();
    registry
        .register_backend(
            "counting",
            Arc::new(CountingFactory {
                producer_calls: Arc::clone(&producer_calls),
                consumer_calls: Arc::clone(&consumer_calls),
            }),
        )
        .unwrap();

    assert_eq!(registry.len(), 1);
    assert_eq!(registry.adapters(), ["counting"]);

    let config = BrokerConfig::new("counting");
    let backend = registry.build(&config).unwrap();
    backend
        .producer
        .send(Message::new("topic", "payload".to_owned()))
        .await
        .unwrap();
    backend.consumer.subscribe(&["topic"]).await.unwrap();

    let _consumer = registry.consumer(&config).unwrap();

    assert_eq!(producer_calls.load(Ordering::SeqCst), 1);
    assert_eq!(consumer_calls.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn stack_builder_applies_middleware_in_added_order() {
    let order = Arc::new(tokio::sync::Mutex::new(Vec::<&'static str>::new()));
    let base_order = Arc::clone(&order);
    let base: Arc<dyn MessageHandler<String>> =
        Arc::new(FnHandler::new(move |_msg: Message<String>| {
            let base_order = Arc::clone(&base_order);
            async move {
                base_order.lock().await.push("base");
                Ok(())
            }
        }));

    let handler = StackBuilder::new(base)
        .with(recording_middleware("outer", Arc::clone(&order)))
        .with(recording_middleware("inner", Arc::clone(&order)))
        .build();

    handler
        .handle(Message::new("topic", "payload".to_owned()))
        .await
        .unwrap();

    assert_eq!(*order.lock().await, ["outer", "inner", "base"]);
}

#[tokio::test]
async fn managed_component_builders_preserve_names_and_custom_backoff() {
    let broker = InMemoryBroker::<String>::new(8);
    let producer = ManagedProducerBuilder::new("producer-a", Arc::new(broker.producer())).build();
    assert_eq!(producer.name(), "producer-a");
    assert!(!producer.is_running());
    assert!(producer.stop().await.is_err());

    let handler: Arc<dyn MessageHandler<String>> =
        Arc::new(FnHandler::new(|_msg: Message<String>| async { Ok(()) }));
    let backoff = RetryPolicy::new()
        .with_linear_backoff(LinearBackoff::new(
            Duration::from_millis(1),
            Duration::from_millis(1),
            Duration::from_millis(5),
        ))
        .with_jitter(false);
    let consumer = ManagedConsumerBuilder::new("consumer-a", Arc::new(broker.consumer()), handler)
        .with_recv_backoff(backoff)
        .build();

    assert_eq!(consumer.name(), "consumer-a");
    assert!(!consumer.is_running());
}

fn recording_middleware(
    label: &'static str,
    order: Arc<tokio::sync::Mutex<Vec<&'static str>>>,
) -> impl HandlerMiddleware<String> {
    middleware_fn(move |next: Arc<dyn MessageHandler<String>>| {
        let order = Arc::clone(&order);
        Arc::new(FnHandler::new(move |msg: Message<String>| {
            let next = Arc::clone(&next);
            let order = Arc::clone(&order);
            async move {
                order.lock().await.push(label);
                next.handle(msg).await
            }
        }))
    })
}

struct CountingFactory {
    producer_calls: Arc<AtomicUsize>,
    consumer_calls: Arc<AtomicUsize>,
}

impl MessagingFactory<String> for CountingFactory {
    fn create_producer(
        &self,
        _config: &BrokerConfig,
    ) -> AppResult<Arc<dyn MessageProducer<String>>> {
        self.producer_calls.fetch_add(1, Ordering::SeqCst);
        Ok(Arc::new(NoopProducer))
    }

    fn create_consumer(
        &self,
        _config: &BrokerConfig,
    ) -> AppResult<Arc<dyn MessageConsumer<String>>> {
        self.consumer_calls.fetch_add(1, Ordering::SeqCst);
        Ok(Arc::new(NoopConsumer))
    }
}

struct NoopProducer;

#[async_trait]
impl MessageProducer<String> for NoopProducer {
    async fn send(&self, _msg: Message<String>) -> AppResult<()> {
        Ok(())
    }

    async fn send_batch(&self, _msgs: Vec<Message<String>>) -> AppResult<()> {
        Ok(())
    }

    async fn flush(&self, _timeout: Duration) -> AppResult<()> {
        Ok(())
    }
}

struct NoopConsumer;

#[async_trait]
impl MessageConsumer<String> for NoopConsumer {
    async fn subscribe(&self, _topics: &[&str]) -> AppResult<()> {
        Ok(())
    }

    async fn recv(&self, timeout: std::time::Duration) -> AppResult<Message<String>> {
        if timeout.is_zero() {
            return Err(AppError::new(
                ErrorCode::InvalidInput,
                "message receive timeout must be greater than zero",
            ));
        }
        Err(AppError::timeout("message receive"))
    }
}
