//! Additional integration-level tests for rskit-messaging public APIs.
//!
//! These tests exercise public APIs via `rskit_messaging::*` imports and
//! complement the 62 inline + 8 integration tests already in the crate.

use rskit_messaging::*;
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
use std::time::Duration;

// ── 1. Message Construction Edge Cases ──────────────────────────────────────

#[tokio::test]
async fn message_with_multiple_headers() {
    let msg = Message::new("topic", "payload".to_string())
        .with_header("h1", "v1")
        .with_header("h2", "v2")
        .with_header("h3", "v3");

    assert_eq!(msg.headers.len(), 3);
    assert_eq!(msg.headers.get("h1").unwrap(), "v1");
    assert_eq!(msg.headers.get("h2").unwrap(), "v2");
    assert_eq!(msg.headers.get("h3").unwrap(), "v3");
}

#[tokio::test]
async fn message_with_message_id_generates_unique_ids() {
    let msg1 = Message::new("t", "a".to_string()).with_message_id();
    let msg2 = Message::new("t", "b".to_string()).with_message_id();

    let id1 = msg1.headers.get("message-id").unwrap();
    let id2 = msg2.headers.get("message-id").unwrap();
    assert_ne!(id1, id2, "Two message IDs should be unique");
    assert!(!id1.is_empty());
    assert!(!id2.is_empty());
}

#[test]
fn message_clone_preserves_all_fields() {
    let msg = Message::new("orders", "data".to_string())
        .with_key("partition-key")
        .with_header("trace-id", "abc-123")
        .with_header("source", "test")
        .with_message_id();

    let cloned = msg.clone();

    assert_eq!(cloned.topic, msg.topic);
    assert_eq!(cloned.key, msg.key);
    assert_eq!(cloned.payload, msg.payload);
    assert_eq!(cloned.headers, msg.headers);
    assert_eq!(cloned.timestamp, msg.timestamp);
    assert_eq!(cloned.partition, msg.partition);
    assert_eq!(cloned.offset, msg.offset);
}

// ── 2. Event Serialization Edge Cases ───────────────────────────────────────

#[test]
fn event_from_json_with_missing_optional_fields() {
    // Minimal JSON: subject defaults to "", content_type to "application/json",
    // version defaults to ""
    let json = serde_json::json!({
        "id": "evt-1",
        "type": "user.created",
        "source": "auth-svc",
        "timestamp": "2024-01-01T00:00:00Z",
        "data": null
    });
    let bytes = serde_json::to_vec(&json).unwrap();
    let event = Event::from_json(&bytes).unwrap();

    assert_eq!(event.id, "evt-1");
    assert_eq!(event.event_type, "user.created");
    assert_eq!(event.source, "auth-svc");
    assert_eq!(event.subject, "");
    assert_eq!(event.content_type, "application/json");
    assert_eq!(event.version, "");
}

#[test]
fn event_from_json_with_invalid_bytes_returns_error() {
    let bad_bytes: &[u8] = &[0xFF, 0xFE, 0xFD];
    let result = Event::from_json(bad_bytes);
    assert!(result.is_err());
}

#[test]
fn event_parse_data_type_mismatch_returns_error() {
    let event = Event::new("test", "src")
        .with_data(serde_json::json!({"name": "test"}))
        .unwrap();

    let result = event.parse_data::<u32>();
    assert!(result.is_err());
}

// ── 3. Router Advanced Patterns ─────────────────────────────────────────────

fn counting_handler(counter: &Arc<AtomicU32>) -> Arc<dyn MessageHandler<String>> {
    let c = counter.clone();
    Arc::new(FnHandler::new(move |_msg: Message<String>| {
        let c = c.clone();
        async move {
            c.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }))
}

#[tokio::test]
async fn router_multiple_wildcards_in_pattern() {
    let counter = Arc::new(AtomicU32::new(0));
    let router = MessageRouter::<String>::new()
        .handle("*.*", counting_handler(&counter))
        .build();

    router
        .handle(Message::new("a.b", "d".to_string()))
        .await
        .unwrap();
    router
        .handle(Message::new("foo.bar", "d".to_string()))
        .await
        .unwrap();

    assert_eq!(counter.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn router_exact_match_takes_priority_over_wildcard_when_registered_first() {
    let exact_counter = Arc::new(AtomicU32::new(0));
    let wildcard_counter = Arc::new(AtomicU32::new(0));

    // Register exact BEFORE wildcard → first match wins
    let router = MessageRouter::<String>::new()
        .handle("orders.created", counting_handler(&exact_counter))
        .handle("orders.*", counting_handler(&wildcard_counter))
        .build();

    router
        .handle(Message::new("orders.created", "d".to_string()))
        .await
        .unwrap();

    assert_eq!(exact_counter.load(Ordering::SeqCst), 1);
    assert_eq!(wildcard_counter.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn router_concurrent_dispatch() {
    let counter = Arc::new(AtomicU32::new(0));
    let router = MessageRouter::<String>::new()
        .handle("events.*", counting_handler(&counter))
        .build();

    let mut tasks = Vec::new();
    for i in 0..10 {
        let r = router.clone();
        tasks.push(tokio::spawn(async move {
            r.handle(Message::new(format!("events.{i}"), "d".to_string()))
                .await
                .unwrap();
        }));
    }

    for t in tasks {
        t.await.unwrap();
    }

    assert_eq!(counter.load(Ordering::SeqCst), 10);
}

// ── 5. Middleware Chain Composition ──────────────────────────────────────────

#[tokio::test]
async fn chain_handlers_empty_middleware_returns_base() {
    let counter = Arc::new(AtomicU32::new(0));
    let c = counter.clone();
    let base: Arc<dyn MessageHandler<String>> =
        Arc::new(FnHandler::new(move |_msg: Message<String>| {
            let c = c.clone();
            async move {
                c.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }
        }));

    let chained = chain_handlers(base, &[]);
    chained
        .handle(Message::new("t", "x".to_string()))
        .await
        .unwrap();

    assert_eq!(counter.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn chain_handlers_multiple_middleware_ordering() {
    let order = Arc::new(tokio::sync::Mutex::new(Vec::<String>::new()));

    let order_a = order.clone();
    let mw_a: Arc<dyn HandlerMiddleware<String>> = Arc::new(middleware_fn(
        move |next: Arc<dyn MessageHandler<String>>| {
            let order_a = order_a.clone();
            let handler: Arc<dyn MessageHandler<String>> =
                Arc::new(FnHandler::new(move |msg: Message<String>| {
                    let next = next.clone();
                    let order_a = order_a.clone();
                    async move {
                        order_a.lock().await.push("A".to_string());
                        next.handle(msg).await
                    }
                }));
            handler
        },
    ));

    let order_b = order.clone();
    let mw_b: Arc<dyn HandlerMiddleware<String>> = Arc::new(middleware_fn(
        move |next: Arc<dyn MessageHandler<String>>| {
            let order_b = order_b.clone();
            let handler: Arc<dyn MessageHandler<String>> =
                Arc::new(FnHandler::new(move |msg: Message<String>| {
                    let next = next.clone();
                    let order_b = order_b.clone();
                    async move {
                        order_b.lock().await.push("B".to_string());
                        next.handle(msg).await
                    }
                }));
            handler
        },
    ));

    let order_base = order.clone();
    let base: Arc<dyn MessageHandler<String>> =
        Arc::new(FnHandler::new(move |_msg: Message<String>| {
            let order_base = order_base.clone();
            async move {
                order_base.lock().await.push("BASE".to_string());
                Ok(())
            }
        }));

    // chain_handlers(base, [mw_a, mw_b]) => mw_a(mw_b(base))
    // Execution: A → B → BASE
    let chained = chain_handlers(base, &[mw_a, mw_b]);
    chained
        .handle(Message::new("t", "x".to_string()))
        .await
        .unwrap();

    let recorded = order.lock().await;
    assert_eq!(*recorded, vec!["A", "B", "BASE"]);
}

#[tokio::test]
async fn middleware_fn_works_as_middleware() {
    let counter = Arc::new(AtomicU32::new(0));
    let mw_counter = counter.clone();

    let mw: Arc<dyn HandlerMiddleware<String>> = Arc::new(middleware_fn(
        move |next: Arc<dyn MessageHandler<String>>| {
            let mw_counter = mw_counter.clone();
            let handler: Arc<dyn MessageHandler<String>> =
                Arc::new(FnHandler::new(move |msg: Message<String>| {
                    let next = next.clone();
                    let mw_counter = mw_counter.clone();
                    async move {
                        mw_counter.fetch_add(1, Ordering::SeqCst);
                        next.handle(msg).await
                    }
                }));
            handler
        },
    ));

    let base_counter = Arc::new(AtomicU32::new(0));
    let bc = base_counter.clone();
    let base: Arc<dyn MessageHandler<String>> =
        Arc::new(FnHandler::new(move |_msg: Message<String>| {
            let bc = bc.clone();
            async move {
                bc.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }
        }));

    let chained = chain_handlers(base, &[mw]);
    chained
        .handle(Message::new("t", "x".to_string()))
        .await
        .unwrap();

    assert_eq!(counter.load(Ordering::SeqCst), 1);
    assert_eq!(base_counter.load(Ordering::SeqCst), 1);
}

// ── 6. Translator Edge Cases ────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct EmptyStruct {}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
struct NestedPayload {
    items: Vec<String>,
    count: Option<u32>,
    metadata: std::collections::HashMap<String, serde_json::Value>,
}

#[test]
fn json_translator_empty_struct() {
    let translator = JsonTranslator;
    let empty = EmptyStruct {};

    let bytes: Vec<u8> =
        MessageTranslator::<Vec<u8>, EmptyStruct>::serialize(&translator, &empty).unwrap();
    let restored: EmptyStruct =
        MessageTranslator::<Vec<u8>, EmptyStruct>::deserialize(&translator, &bytes).unwrap();
    assert_eq!(restored, empty);
}

#[test]
fn json_string_translator_bad_input_returns_error() {
    let translator = JsonStringTranslator;
    let bad_json = "not valid json{{{".to_string();

    let result: rskit_errors::AppResult<EmptyStruct> =
        MessageTranslator::<String, EmptyStruct>::deserialize(&translator, &bad_json);
    assert!(result.is_err());
}

#[test]
fn json_translator_nested_types() {
    let translator = JsonTranslator;

    let mut metadata = std::collections::HashMap::new();
    metadata.insert("region".to_string(), serde_json::json!("us-east-1"));
    metadata.insert("tags".to_string(), serde_json::json!(["prod", "v2"]));

    let payload = NestedPayload {
        items: vec!["alpha".into(), "beta".into(), "gamma".into()],
        count: Some(42),
        metadata,
    };

    let bytes: Vec<u8> =
        MessageTranslator::<Vec<u8>, NestedPayload>::serialize(&translator, &payload).unwrap();
    let restored: NestedPayload =
        MessageTranslator::<Vec<u8>, NestedPayload>::deserialize(&translator, &bytes).unwrap();
    assert_eq!(restored, payload);
}

// ── 7. Batch Producer Advanced ──────────────────────────────────────────────

#[tokio::test]
async fn batch_manual_flush() {
    let broker = InMemoryBroker::<String>::new(64);
    let _consumer = broker.consumer();
    let producer = broker.producer();

    let batch = BatchProducer::new(
        Arc::new(producer),
        "batch-topic".to_string(),
        BatchConfig {
            max_size: 100,
            max_wait: Duration::from_secs(60),
            max_bytes: None,
        },
    );

    batch
        .send(Message::new("batch-topic", "a".to_string()))
        .await
        .unwrap();
    batch
        .send(Message::new("batch-topic", "b".to_string()))
        .await
        .unwrap();

    // Not yet flushed by size
    assert_eq!(broker.message_count("batch-topic").await, 0);

    // Manual flush
    batch.flush().await.unwrap();
    assert_eq!(broker.message_count("batch-topic").await, 2);

    batch.close().await.unwrap();
}

#[tokio::test]
async fn batch_multiple_size_flushes() {
    let broker = InMemoryBroker::<String>::new(64);
    let _consumer = broker.consumer();
    let producer = broker.producer();

    let batch = BatchProducer::new(
        Arc::new(producer),
        "batch-topic".to_string(),
        BatchConfig {
            max_size: 2,
            max_wait: Duration::from_secs(60),
            max_bytes: None,
        },
    );

    // First batch of 2
    batch
        .send(Message::new("batch-topic", "1".to_string()))
        .await
        .unwrap();
    batch
        .send(Message::new("batch-topic", "2".to_string()))
        .await
        .unwrap();
    assert_eq!(broker.message_count("batch-topic").await, 2);

    // Second batch of 2
    batch
        .send(Message::new("batch-topic", "3".to_string()))
        .await
        .unwrap();
    batch
        .send(Message::new("batch-topic", "4".to_string()))
        .await
        .unwrap();
    assert_eq!(broker.message_count("batch-topic").await, 4);

    batch.close().await.unwrap();
}

#[tokio::test]
async fn batch_close_is_idempotent() {
    let broker = InMemoryBroker::<String>::new(64);
    let _consumer = broker.consumer();
    let producer = broker.producer();

    let batch = BatchProducer::new(
        Arc::new(producer),
        "batch-topic".to_string(),
        BatchConfig {
            max_size: 100,
            max_wait: Duration::from_secs(60),
            max_bytes: None,
        },
    );

    batch
        .send(Message::new("batch-topic", "x".to_string()))
        .await
        .unwrap();

    batch.close().await.unwrap();
    // Second close should not panic
    batch.close().await.unwrap();
}

// ── 8. Managed Components Advanced ──────────────────────────────────────────

#[tokio::test]
async fn managed_producer_send_batch_while_running() {
    let broker = InMemoryBroker::<String>::new(64);
    let _consumer = broker.consumer();
    let inner = broker.producer();

    let producer = ManagedProducerBuilder::new("batch-test", Arc::new(inner)).build();
    producer.start().unwrap();

    let msgs = vec![
        Message::new("t", "a".to_string()),
        Message::new("t", "b".to_string()),
        Message::new("t", "c".to_string()),
    ];
    producer.send_batch(msgs).await.unwrap();

    assert_eq!(broker.message_count("t").await, 3);
    producer.stop().await.unwrap();
}

#[tokio::test]
async fn managed_producer_flush_while_stopped_returns_error() {
    let broker = InMemoryBroker::<String>::new(64);
    let inner = broker.producer();

    let producer = ManagedProducerBuilder::new("flush-test", Arc::new(inner)).build();

    // flush without start → error
    let result = producer.flush(Duration::from_secs(1)).await;
    assert!(result.is_err());
}

// ── 9. Error Classification ─────────────────────────────────────────────────

#[test]
fn error_classifier_trait_is_object_safe() {
    let classifier: Box<dyn ErrorClassifier> = Box::new(NoopErrorClassifier);
    let err = rskit_errors::AppError::new(rskit_errors::ErrorCode::Internal, "test error");

    assert!(!classifier.is_connection_error(&err));
    assert!(!classifier.is_retryable_error(&err));
}

#[test]
fn noop_error_classifier_default() {
    let _classifier = NoopErrorClassifier;
    // Verify Default trait works via the type system
    let classifier: NoopErrorClassifier = Default::default();
    let err = rskit_errors::AppError::new(rskit_errors::ErrorCode::Internal, "test");
    assert!(!classifier.is_connection_error(&err));
    assert!(!classifier.is_retryable_error(&err));
}

#[test]
fn core_registry_starts_empty_for_binary_payloads() {
    let registry = rskit_messaging::MessagingRegistry::<Vec<u8>>::new();
    assert!(registry.producer_adapters().is_empty());
    assert!(registry.consumer_adapters().is_empty());
    assert!(registry.producer("kafka").is_err());
    assert!(registry.producer("nats").is_err());
    assert!(registry.producer("rabbitmq").is_err());
}

#[test]
fn core_manifest_has_no_broker_sdk_dependencies_or_features() {
    let manifest = include_str!("../Cargo.toml");
    assert!(!manifest.contains("rdkafka"));
    assert!(!manifest.contains("async-nats"));
    assert!(!manifest.contains("lapin"));
    assert!(!manifest.contains("kafka ="));
    assert!(!manifest.contains("nats ="));
    assert!(!manifest.contains("rabbitmq ="));
}
