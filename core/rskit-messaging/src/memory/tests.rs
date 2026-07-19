//! Behavior tests for the in-memory broker, producer, consumer, and helpers.

use std::time::Duration;

use crate::config::BrokerConfig;
use crate::event::Event;
use crate::message::Message;
use crate::registry::MessagingRegistry;
use crate::traits::{EventConsumer, EventProducer, MessageConsumer, MessageProducer};

use super::*;

#[tokio::test]
async fn send_and_receive() {
    let broker: InMemoryBroker<String> = InMemoryBroker::new(16);
    let producer = broker.producer();
    let consumer = broker.consumer();

    consumer.subscribe(&["test-topic"]).await.unwrap();

    let msg = Message::new("test-topic", "hello".to_string());
    producer.send(msg).await.unwrap();

    let received = consumer
        .recv(std::time::Duration::from_secs(1))
        .await
        .unwrap();
    assert_eq!(received.topic, "test-topic");
    assert_eq!(received.payload, "hello");
}

#[tokio::test]
async fn register_memory_adapter_explicitly() {
    let broker: InMemoryBroker<String> = InMemoryBroker::new(16);
    let mut registry = MessagingRegistry::new();

    register(&mut registry, broker).unwrap();

    assert_eq!(registry.adapters(), vec!["memory"]);
    let config = BrokerConfig::default();
    let producer = registry.producer(&config).unwrap();
    let consumer = registry.consumer(&config).unwrap();
    consumer.subscribe(&["events"]).await.unwrap();
    producer
        .send(Message::new("events", "registered".to_string()))
        .await
        .unwrap();
    let received = consumer
        .recv(std::time::Duration::from_secs(1))
        .await
        .unwrap();
    assert_eq!(received.payload, "registered");
}

#[tokio::test]
async fn send_batch_and_receive() {
    let broker: InMemoryBroker<i32> = InMemoryBroker::new(16);
    let producer = broker.producer();
    let consumer = broker.consumer();

    consumer.subscribe(&["numbers"]).await.unwrap();

    let msgs = vec![
        Message::new("numbers", 1),
        Message::new("numbers", 2),
        Message::new("numbers", 3),
    ];
    producer.send_batch(msgs).await.unwrap();

    let a = consumer
        .recv(std::time::Duration::from_secs(1))
        .await
        .unwrap();
    let b = consumer
        .recv(std::time::Duration::from_secs(1))
        .await
        .unwrap();
    let c = consumer
        .recv(std::time::Duration::from_secs(1))
        .await
        .unwrap();
    assert_eq!(a.payload, 1);
    assert_eq!(b.payload, 2);
    assert_eq!(c.payload, 3);
}

#[tokio::test]
async fn topic_filtering() {
    let broker: InMemoryBroker<String> = InMemoryBroker::new(16);
    let producer = broker.producer();
    let consumer = broker.consumer();

    consumer.subscribe(&["wanted"]).await.unwrap();

    producer
        .send(Message::new("ignored", "nope".to_string()))
        .await
        .unwrap();
    producer
        .send(Message::new("wanted", "yes".to_string()))
        .await
        .unwrap();

    let received = consumer
        .recv(std::time::Duration::from_secs(1))
        .await
        .unwrap();
    assert_eq!(received.topic, "wanted");
    assert_eq!(received.payload, "yes");
}

#[tokio::test]
async fn flush_is_noop() {
    let broker: InMemoryBroker<()> = InMemoryBroker::new(4);
    let producer = broker.producer();
    producer.flush(Duration::from_secs(1)).await.unwrap();
}

#[tokio::test]
async fn event_publish_and_receive() {
    let broker: InMemoryBroker<serde_json::Value> = InMemoryBroker::new(16);
    let producer = broker.producer();
    let consumer = broker.consumer();

    EventConsumer::subscribe(&consumer, &["events"])
        .await
        .unwrap();

    let event = Event::new("user.created", "auth-service")
        .with_subject("user-42")
        .with_data(serde_json::json!({"name": "Alice"}))
        .unwrap();
    let original_id = event.id.clone();

    producer.publish("events", event).await.unwrap();

    let received = consumer
        .recv_event(std::time::Duration::from_secs(1))
        .await
        .unwrap();
    assert_eq!(received.id, original_id);
    assert_eq!(received.event_type, "user.created");
    assert_eq!(received.source, "auth-service");
    assert_eq!(received.subject, "user-42");
    assert_eq!(received.data, serde_json::json!({"name": "Alice"}));
}

#[tokio::test]
async fn event_publish_batch_and_receive() {
    let broker: InMemoryBroker<serde_json::Value> = InMemoryBroker::new(16);
    let producer = broker.producer();
    let consumer = broker.consumer();

    EventConsumer::subscribe(&consumer, &["batch"])
        .await
        .unwrap();

    let events = vec![
        Event::new("a", "src"),
        Event::new("b", "src"),
        Event::new("c", "src"),
    ];
    producer.publish_batch("batch", events).await.unwrap();

    let a = consumer
        .recv_event(std::time::Duration::from_secs(1))
        .await
        .unwrap();
    let b = consumer
        .recv_event(std::time::Duration::from_secs(1))
        .await
        .unwrap();
    let c = consumer
        .recv_event(std::time::Duration::from_secs(1))
        .await
        .unwrap();
    assert_eq!(a.event_type, "a");
    assert_eq!(b.event_type, "b");
    assert_eq!(c.event_type, "c");
}

// ── History & topic helper tests ────────────────────────────────────────

#[tokio::test]
async fn messages_returns_topic_history() {
    let broker: InMemoryBroker<String> = InMemoryBroker::new(16);
    let producer = broker.producer();
    // Need a consumer so broadcast::send succeeds.
    let _consumer = broker.consumer();

    producer
        .send(Message::new("t1", "a".to_string()))
        .await
        .unwrap();
    producer
        .send(Message::new("t1", "b".to_string()))
        .await
        .unwrap();
    producer
        .send(Message::new("t2", "c".to_string()))
        .await
        .unwrap();

    let t1 = broker.messages("t1").await;
    assert_eq!(t1.len(), 2);
    assert_eq!(t1[0].payload, "a");
    assert_eq!(t1[1].payload, "b");

    let all = broker.all_messages().await;
    assert_eq!(all.len(), 3);
}

#[tokio::test]
async fn in_memory_history_is_bounded() {
    let broker = InMemoryBroker::with_history_limit(8, 2);
    let producer = broker.producer();
    let _consumer = broker.consumer();

    producer.send(Message::new("events", 1_u32)).await.unwrap();
    producer.send(Message::new("events", 2_u32)).await.unwrap();
    producer.send(Message::new("events", 3_u32)).await.unwrap();

    let messages = broker.messages("events").await;
    assert_eq!(messages.len(), 2);
    assert_eq!(messages[0].payload, 2);
    assert_eq!(messages[1].payload, 3);
}

#[tokio::test]
async fn message_count_and_reset() {
    let broker: InMemoryBroker<i32> = InMemoryBroker::new(16);
    let producer = broker.producer();
    let _consumer = broker.consumer();

    assert_eq!(broker.message_count("t").await, 0);
    producer.send(Message::new("t", 1)).await.unwrap();
    assert_eq!(broker.message_count("t").await, 1);

    broker.reset().await;
    assert_eq!(broker.message_count("t").await, 0);
}

#[tokio::test]
async fn create_topic_and_topic_names() {
    let broker: InMemoryBroker<String> = InMemoryBroker::new(16);
    let _consumer = broker.consumer();

    broker.create_topic("z-topic").await;
    broker.create_topic("a-topic").await;

    producer_send_helper(&broker, "m-topic").await;

    let names = broker.topic_names().await;
    assert_eq!(names, vec!["a-topic", "m-topic", "z-topic"]);
}

/// Helper: send a dummy message so that the topic appears in history.
async fn producer_send_helper(broker: &InMemoryBroker<String>, topic: &str) {
    let producer = broker.producer();
    producer
        .send(Message::new(topic, "x".to_string()))
        .await
        .unwrap();
}

// ── Assertion helper tests ──────────────────────────────────────────────

#[tokio::test]
async fn test_assert_published() {
    let broker: InMemoryBroker<String> = InMemoryBroker::new(16);
    let producer = broker.producer();
    let _consumer = broker.consumer();

    producer
        .send(Message::new("t1", "hello".to_string()))
        .await
        .unwrap();
    producer
        .send(Message::new("t1", "world".to_string()))
        .await
        .unwrap();

    assert_published(&broker, "t1", |m| m.payload == "world").await;
}

#[tokio::test]
async fn test_assert_published_n() {
    let broker: InMemoryBroker<String> = InMemoryBroker::new(16);
    let producer = broker.producer();
    let _consumer = broker.consumer();

    producer
        .send(Message::new("t1", "a".to_string()))
        .await
        .unwrap();
    producer
        .send(Message::new("t1", "b".to_string()))
        .await
        .unwrap();

    assert_published_n(&broker, "t1", 2).await;
}

#[tokio::test]
async fn test_assert_no_messages() {
    let broker: InMemoryBroker<String> = InMemoryBroker::new(16);
    assert_no_messages(&broker, "empty-topic").await;
}

#[tokio::test]
async fn test_wait_for_message() {
    let broker: InMemoryBroker<String> = InMemoryBroker::new(16);
    let _consumer = broker.consumer();

    let broker_clone = broker.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(20)).await;
        let producer = broker_clone.producer();
        producer
            .send(Message::new("t1", "delayed".to_string()))
            .await
            .unwrap();
    });

    let msg = wait_for_message(&broker, "t1", Duration::from_secs(2)).await;
    assert_eq!(msg.payload, "delayed");
}
#[tokio::test]
async fn default_history_is_bounded_by_capacity() {
    let broker: InMemoryBroker<usize> = InMemoryBroker::new(8);
    let producer = broker.producer();
    let _consumer = broker.consumer();

    for value in 0..1030 {
        producer.send(Message::new("history", value)).await.unwrap();
    }

    let messages = broker.messages("history").await;
    assert_eq!(messages.len(), 8);
    assert_eq!(messages.first().map(|msg| msg.payload), Some(1022));
}

#[tokio::test]
async fn bounded_history_limit_is_opt_in() {
    let broker: InMemoryBroker<usize> = InMemoryBroker::with_history_limit(8, 2);
    let producer = broker.producer();
    let _consumer = broker.consumer();

    for value in 0..4 {
        producer.send(Message::new("history", value)).await.unwrap();
    }

    let payloads = broker
        .messages("history")
        .await
        .into_iter()
        .map(|msg| msg.payload)
        .collect::<Vec<_>>();
    assert_eq!(payloads, vec![2, 3]);
}

#[tokio::test]
async fn zero_capacity_is_clamped() {
    let broker: InMemoryBroker<usize> = InMemoryBroker::new(0);
    let producer = broker.producer();
    let _consumer = broker.consumer();

    producer.send(Message::new("history", 1)).await.unwrap();

    let messages = broker.messages("history").await;
    assert_eq!(messages.len(), 1);
}
