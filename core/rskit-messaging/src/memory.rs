//! In-memory message broker, producer, and consumer for testing.

use std::collections::{HashSet, VecDeque};
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use rskit_errors::{AppError, AppResult, ErrorCode};
use tokio::sync::{Mutex, broadcast};

use crate::config::BrokerConfig;
use crate::event::Event;
use crate::message::Message;
use crate::registry::{MessagingFactory, MessagingRegistry};
use crate::traits::{EventConsumer, EventProducer, MessageConsumer, MessageProducer};

const ADAPTER_NAME: &str = "memory";

/// An in-memory message broker backed by a `tokio::sync::broadcast` channel.
///
/// Create one broker and hand out producers / consumers via
/// [`InMemoryBroker::producer`] and [`InMemoryBroker::consumer`].
///
/// Every message sent through the broker is recorded in an internal history
/// so that test assertion helpers can inspect what was published.
#[derive(Debug, Clone)]
pub struct InMemoryBroker<T: Clone + Send + Sync + 'static> {
    tx: broadcast::Sender<Message<T>>,
    history: Arc<Mutex<VecDeque<Message<T>>>>,
    history_limit: Option<usize>,
    topics: Arc<Mutex<HashSet<String>>>,
    /// Notified after every publish so that [`wait_for_message`] can wake
    /// without polling.
    notify: Arc<tokio::sync::Notify>,
}

impl<T: Clone + Send + Sync + 'static> InMemoryBroker<T> {
    /// Create a broker with the given channel capacity.
    pub fn new(capacity: usize) -> Self {
        let limit = capacity.max(1);
        let (tx, _) = broadcast::channel(limit);
        Self {
            tx,
            history: Arc::new(Mutex::new(VecDeque::with_capacity(limit))),
            history_limit: Some(limit),
            topics: Arc::new(Mutex::new(HashSet::new())),
            notify: Arc::new(tokio::sync::Notify::new()),
        }
    }

    /// Create a producer attached to this broker.
    pub fn producer(&self) -> InMemoryProducer<T> {
        InMemoryProducer {
            tx: self.tx.clone(),
            history: self.history.clone(),
            history_limit: self.history_limit,
            topics: self.topics.clone(),
            notify: self.notify.clone(),
        }
    }

    /// Create a broker with explicit channel capacity and bounded history limit.
    ///
    /// The default [`InMemoryBroker::new`] bounds history by channel capacity. Use this
    /// constructor when tests need a different history limit.
    #[must_use]
    pub fn with_history_limit(capacity: usize, history_limit: usize) -> Self {
        let capacity = capacity.max(1);
        let limit = history_limit.max(1);
        let (tx, _) = broadcast::channel(capacity);
        Self {
            tx,
            history: Arc::new(Mutex::new(VecDeque::with_capacity(limit))),
            history_limit: Some(limit),
            topics: Arc::new(Mutex::new(HashSet::new())),
            notify: Arc::new(tokio::sync::Notify::new()),
        }
    }

    /// Create a consumer attached to this broker.
    pub fn consumer(&self) -> InMemoryConsumer<T> {
        InMemoryConsumer {
            rx: Arc::new(Mutex::new(self.tx.subscribe())),
            topics: Arc::new(Mutex::new(HashSet::new())),
        }
    }

    /// Return a clone of all messages published to `topic`.
    pub async fn messages(&self, topic: &str) -> Vec<Message<T>> {
        self.history
            .lock()
            .await
            .iter()
            .filter(|m| m.topic == topic)
            .cloned()
            .collect()
    }

    /// Return a clone of every message published to any topic.
    pub async fn all_messages(&self) -> Vec<Message<T>> {
        self.history.lock().await.iter().cloned().collect()
    }

    /// Return the number of messages published to `topic`.
    pub async fn message_count(&self, topic: &str) -> usize {
        self.history
            .lock()
            .await
            .iter()
            .filter(|m| m.topic == topic)
            .count()
    }

    /// Clear the recorded message history.
    pub async fn reset(&self) {
        self.history.lock().await.clear();
    }

    /// Pre-create a topic so that it appears in [`InMemoryBroker::topic_names`].
    pub async fn create_topic(&self, topic: &str) {
        self.topics.lock().await.insert(topic.to_string());
    }

    /// Return the sorted set of topic names that have been created or published to.
    pub async fn topic_names(&self) -> Vec<String> {
        let explicit = self.topics.lock().await;
        let hist = self.history.lock().await;
        let mut set: HashSet<String> = explicit.clone();
        for m in hist.iter() {
            set.insert(m.topic.clone());
        }
        let mut out: Vec<String> = set.into_iter().collect();
        out.sort();
        out
    }
}

impl<T: Clone + Send + Sync + 'static> Default for InMemoryBroker<T> {
    fn default() -> Self {
        Self::new(256)
    }
}

/// Register in-memory producer and consumer factories.
pub fn register<T: Clone + Send + Sync + 'static>(
    registry: &mut MessagingRegistry<T>,
    broker: InMemoryBroker<T>,
) -> AppResult<()> {
    registry.register_backend(ADAPTER_NAME, Arc::new(MemoryFactory { broker }))
}

struct MemoryFactory<T: Clone + Send + Sync + 'static> {
    broker: InMemoryBroker<T>,
}

impl<T: Clone + Send + Sync + 'static> MessagingFactory<T> for MemoryFactory<T> {
    fn create_producer(&self, _config: &BrokerConfig) -> AppResult<Arc<dyn MessageProducer<T>>> {
        Ok(Arc::new(self.broker.producer()))
    }

    fn create_consumer(&self, _config: &BrokerConfig) -> AppResult<Arc<dyn MessageConsumer<T>>> {
        Ok(Arc::new(self.broker.consumer()))
    }
}

/// An in-memory message producer.
#[derive(Debug, Clone)]
pub struct InMemoryProducer<T: Clone + Send + Sync + 'static> {
    tx: broadcast::Sender<Message<T>>,
    history: Arc<Mutex<VecDeque<Message<T>>>>,
    history_limit: Option<usize>,
    topics: Arc<Mutex<HashSet<String>>>,
    notify: Arc<tokio::sync::Notify>,
}

#[async_trait]
impl<T: Clone + Send + Sync + 'static> MessageProducer<T> for InMemoryProducer<T> {
    async fn send(&self, msg: Message<T>) -> AppResult<()> {
        // Record in history before broadcasting.
        {
            let mut hist = self.history.lock().await;
            if let Some(limit) = self.history_limit
                && hist.len() == limit
            {
                hist.pop_front();
            }
            hist.push_back(msg.clone());
        }
        {
            let mut set = self.topics.lock().await;
            set.insert(msg.topic.clone());
        }

        self.tx.send(msg).map_err(|_| {
            AppError::new(ErrorCode::ExternalService, "no active consumers on channel")
        })?;

        self.notify.notify_waiters();
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

/// Assert that at least one message on `topic` satisfies the predicate.
///
/// # Panics
///
/// Panics when no matching message is found.
pub async fn assert_published<T: Clone + Send + Sync + 'static>(
    broker: &InMemoryBroker<T>,
    topic: &str,
    predicate: impl Fn(&Message<T>) -> bool,
) {
    let msgs = broker.messages(topic).await;
    assert!(
        msgs.iter().any(&predicate),
        "assert_published: no message on topic {topic:?} matched the predicate ({} checked)",
        msgs.len(),
    );
}

/// Assert that exactly `n` messages were published to `topic`.
///
/// # Panics
///
/// Panics when the count does not match.
pub async fn assert_published_n<T: Clone + Send + Sync + 'static>(
    broker: &InMemoryBroker<T>,
    topic: &str,
    n: usize,
) {
    let got = broker.message_count(topic).await;
    assert_eq!(
        got, n,
        "assert_published_n: topic {topic:?} has {got} messages, want {n}",
    );
}

/// Wait until at least one message appears on `topic` or the timeout expires.
///
/// Returns the first message on the topic.
///
/// # Panics
///
/// Panics if the timeout elapses before any message arrives.
pub async fn wait_for_message<T: Clone + Send + Sync + 'static>(
    broker: &InMemoryBroker<T>,
    topic: &str,
    timeout: Duration,
) -> Message<T> {
    let deadline = tokio::time::Instant::now() + timeout;

    loop {
        let msgs = broker.messages(topic).await;
        if let Some(m) = msgs.into_iter().next() {
            return m;
        }
        tokio::select! {
            _ = broker.notify.notified() => { /* re-check */ }
            _ = tokio::time::sleep_until(deadline) => {
                panic!("wait_for_message: timed out after {timeout:?} waiting for message on topic {topic:?}");
            }
        }
    }
}

/// Assert that zero messages were published to `topic`.
///
/// # Panics
///
/// Panics when the topic is not empty.
pub async fn assert_no_messages<T: Clone + Send + Sync + 'static>(
    broker: &InMemoryBroker<T>,
    topic: &str,
) {
    let n = broker.message_count(topic).await;
    assert_eq!(
        n, 0,
        "assert_no_messages: topic {topic:?} has {n} messages, want 0",
    );
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
        let received = consumer.recv().await.unwrap();
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
}
