//! In-memory message broker, producer, and consumer for testing.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use rskit_errors::{AppError, AppResult, ErrorCode};
use tokio::sync::{Mutex, broadcast};

use crate::event::Event;
use crate::message::Message;
use crate::traits::{EventConsumer, EventProducer, MessageConsumer, MessageProducer};

/// An in-memory message broker backed by a `tokio::sync::broadcast` channel.
///
/// Create one broker and hand out producers / consumers via
/// [`InMemoryBroker::producer`] and [`InMemoryBroker::consumer`].
#[derive(Debug, Clone)]
pub struct InMemoryBroker<T: Clone + Send + Sync + 'static> {
    tx: broadcast::Sender<Message<T>>,
}

impl<T: Clone + Send + Sync + 'static> InMemoryBroker<T> {
    /// Create a broker with the given channel capacity.
    pub fn new(capacity: usize) -> Self {
        let (tx, _) = broadcast::channel(capacity);
        Self { tx }
    }

    /// Create a producer attached to this broker.
    pub fn producer(&self) -> InMemoryProducer<T> {
        InMemoryProducer {
            tx: self.tx.clone(),
        }
    }

    /// Create a consumer attached to this broker.
    pub fn consumer(&self) -> InMemoryConsumer<T> {
        InMemoryConsumer {
            rx: Arc::new(Mutex::new(self.tx.subscribe())),
            topics: Arc::new(Mutex::new(HashSet::new())),
        }
    }
}

impl<T: Clone + Send + Sync + 'static> Default for InMemoryBroker<T> {
    fn default() -> Self {
        Self::new(256)
    }
}

/// An in-memory message producer.
#[derive(Debug, Clone)]
pub struct InMemoryProducer<T: Clone + Send + Sync + 'static> {
    tx: broadcast::Sender<Message<T>>,
}

#[async_trait]
impl<T: Clone + Send + Sync + 'static> MessageProducer<T> for InMemoryProducer<T> {
    async fn send(&self, msg: Message<T>) -> AppResult<()> {
        self.tx.send(msg).map_err(|_| {
            AppError::new(ErrorCode::ExternalService, "no active consumers on channel")
        })?;
        Ok(())
    }

    async fn send_batch(&self, msgs: Vec<Message<T>>) -> AppResult<()> {
        for msg in msgs {
            self.send(msg).await?;
        }
        Ok(())
    }

    async fn flush(&self, _timeout: Duration) -> AppResult<()> {
        // In-memory delivery is instant; nothing to flush.
        Ok(())
    }
}

/// An in-memory message consumer.
#[derive(Debug)]
pub struct InMemoryConsumer<T: Clone + Send + Sync + 'static> {
    rx: Arc<Mutex<broadcast::Receiver<Message<T>>>>,
    topics: Arc<Mutex<HashSet<String>>>,
}

// Manual Clone because broadcast::Receiver is not Clone but we can resubscribe.
impl<T: Clone + Send + Sync + 'static> Clone for InMemoryConsumer<T> {
    fn clone(&self) -> Self {
        Self {
            rx: self.rx.clone(),
            topics: self.topics.clone(),
        }
    }
}

#[async_trait]
impl<T: Clone + Send + Sync + 'static> MessageConsumer<T> for InMemoryConsumer<T> {
    async fn subscribe(&self, topics: &[&str]) -> AppResult<()> {
        let mut set = self.topics.lock().await;
        for t in topics {
            set.insert((*t).to_string());
        }
        Ok(())
    }

    async fn recv(&self) -> AppResult<Message<T>> {
        loop {
            let msg = {
                let mut rx = self.rx.lock().await;
                rx.recv().await.map_err(|e| {
                    AppError::new(ErrorCode::ExternalService, format!("receive failed: {e}"))
                })?
            };

            let topics = self.topics.lock().await;
            // If no explicit subscription, accept all messages.
            if topics.is_empty() || topics.contains(&msg.topic) {
                return Ok(msg);
            }
            // Otherwise loop to skip messages for other topics.
        }
    }
}

#[async_trait]
impl EventProducer for InMemoryProducer<serde_json::Value> {
    async fn publish(&self, topic: &str, event: Event) -> AppResult<()> {
        let value = serde_json::to_value(&event).map_err(|e| {
            AppError::new(
                ErrorCode::Internal,
                format!("Failed to serialize event: {e}"),
            )
        })?;
        self.send(Message::new(topic, value)).await
    }

    async fn publish_batch(&self, topic: &str, events: Vec<Event>) -> AppResult<()> {
        for event in events {
            self.publish(topic, event).await?;
        }
        Ok(())
    }
}

#[async_trait]
impl EventConsumer for InMemoryConsumer<serde_json::Value> {
    async fn subscribe(&self, topics: &[&str]) -> AppResult<()> {
        MessageConsumer::subscribe(self, topics).await
    }

    async fn recv_event(&self) -> AppResult<Event> {
        let msg = self.recv().await?;
        serde_json::from_value(msg.payload).map_err(|e| {
            AppError::new(
                ErrorCode::Internal,
                format!("Failed to deserialize event: {e}"),
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn send_and_receive() {
        let broker: InMemoryBroker<String> = InMemoryBroker::new(16);
        let producer = broker.producer();
        let consumer = broker.consumer();

        consumer.subscribe(&["test-topic"]).await.unwrap();

        let msg = Message::new("test-topic", "hello".to_string());
        producer.send(msg).await.unwrap();

        let received = consumer.recv().await.unwrap();
        assert_eq!(received.topic, "test-topic");
        assert_eq!(received.payload, "hello");
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

        let a = consumer.recv().await.unwrap();
        let b = consumer.recv().await.unwrap();
        let c = consumer.recv().await.unwrap();
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

        let received = consumer.recv().await.unwrap();
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

        let received = consumer.recv_event().await.unwrap();
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

        let a = consumer.recv_event().await.unwrap();
        let b = consumer.recv_event().await.unwrap();
        let c = consumer.recv_event().await.unwrap();
        assert_eq!(a.event_type, "a");
        assert_eq!(b.event_type, "b");
        assert_eq!(c.event_type, "c");
    }
}
