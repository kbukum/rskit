//! Integration for `rskit-messaging`.

use rskit_messaging::{InMemoryBroker, Message, MessageConsumer, MessageProducer};
use std::time::Duration;

#[tokio::test]
async fn broker_creates_producer_and_consumer() {
    let broker = InMemoryBroker::<String>::new(16);
    let _producer = broker.producer();
    let _consumer = broker.consumer();
}

#[tokio::test]
async fn send_and_receive_message() {
    let broker = InMemoryBroker::<String>::new(16);
    let producer = broker.producer();
    let consumer = broker.consumer();

    consumer.subscribe(&["greetings"]).await.unwrap();

    let msg = Message::new("greetings", "hello world".to_string());
    producer.send(msg).await.unwrap();

    let received = consumer
        .recv(std::time::Duration::from_secs(1))
        .await
        .unwrap();
    assert_eq!(received.topic, "greetings");
    assert_eq!(received.payload, "hello world");
}

#[tokio::test]
async fn message_has_correct_topic_and_payload() {
    let msg = Message::new("events", 42u64);
    assert_eq!(msg.topic, "events");
    assert_eq!(msg.payload, 42u64);
    assert!(msg.key.is_none());
    assert!(msg.headers.is_empty());
}

#[tokio::test]
async fn message_builder_methods() {
    let msg = Message::new("t", "data".to_string())
        .with_key("k1")
        .with_header("h1", "v1")
        .with_message_id();

    assert_eq!(msg.key.as_deref(), Some("k1"));
    assert_eq!(msg.headers.get("h1").unwrap(), "v1");
    assert!(msg.headers.contains_key("message-id"));
}

#[tokio::test]
async fn multiple_messages_maintain_ordering() {
    let broker = InMemoryBroker::<String>::new(16);
    let producer = broker.producer();
    let consumer = broker.consumer();

    consumer.subscribe(&["topic"]).await.unwrap();

    for i in 0..5 {
        let msg = Message::new("topic", format!("msg-{i}"));
        producer.send(msg).await.unwrap();
    }

    for i in 0..5 {
        let received = consumer
            .recv(std::time::Duration::from_secs(1))
            .await
            .unwrap();
        assert_eq!(received.payload, format!("msg-{i}"));
    }
}

#[tokio::test]
async fn send_batch_delivers_all_messages() {
    let broker = InMemoryBroker::<String>::new(16);
    let producer = broker.producer();
    let consumer = broker.consumer();

    consumer.subscribe(&["batch"]).await.unwrap();

    let msgs = vec![
        Message::new("batch", "a".to_string()),
        Message::new("batch", "b".to_string()),
        Message::new("batch", "c".to_string()),
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
    assert_eq!(a.payload, "a");
    assert_eq!(b.payload, "b");
    assert_eq!(c.payload, "c");
}

#[tokio::test]
async fn flush_succeeds() {
    let broker = InMemoryBroker::<String>::new(16);
    let producer = broker.producer();
    producer.flush(Duration::from_secs(1)).await.unwrap();
}

#[tokio::test]
async fn consumer_filters_by_subscribed_topic() {
    let broker = InMemoryBroker::<String>::new(16);
    let producer = broker.producer();
    let consumer = broker.consumer();

    consumer.subscribe(&["wanted"]).await.unwrap();

    producer
        .send(Message::new("unwanted", "skip".to_string()))
        .await
        .unwrap();
    producer
        .send(Message::new("wanted", "keep".to_string()))
        .await
        .unwrap();

    let received = consumer
        .recv(std::time::Duration::from_secs(1))
        .await
        .unwrap();
    assert_eq!(received.topic, "wanted");
    assert_eq!(received.payload, "keep");
}
