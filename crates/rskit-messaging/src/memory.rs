//! In-memory message broker, producer, and consumer for testing.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use rskit_errors::{AppError, AppResult, ErrorCode};
use tokio::sync::{Mutex, broadcast};

use crate::message::Message;
use crate::traits::{MessageConsumer, MessageProducer};

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
                    AppError::new(
                        ErrorCode::ExternalService,
                        format!("receive failed: {e}"),
                    )
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
        producer
            .flush(Duration::from_secs(1))
            .await
            .unwrap();
    }
}
