//! High-level event publishing facade.
//!
//! [`EventPublisher`] wraps any [`EventProducer`] with a pre-configured `source` name,
//! so callers only need to provide the topic, event type, and payload —
//! the envelope (ID, timestamp, version) is filled in automatically.
//!
//! # Example
//!
//! ```rust,no_run
//! use rskit_messaging::{EventPublisher, EventProducer, Event};
//! use rskit_errors::AppResult;
//! use serde::Serialize;
//!
//! #[derive(Serialize)]
//! struct OrderPlaced { order_id: String, total: f64 }
//!
//! async fn example(producer: impl EventProducer) -> AppResult<()> {
//!     let publisher = EventPublisher::new(producer, "order-service");
//!
//!     publisher.publish(
//!         "orders.placed",
//!         "order.placed",
//!         &OrderPlaced { order_id: "abc".into(), total: 99.99 },
//!     ).await?;
//!
//!     // With a partition key:
//!     publisher.publish_keyed(
//!         "orders.placed",
//!         "order.placed",
//!         &OrderPlaced { order_id: "def".into(), total: 42.0 },
//!         "def",
//!     ).await
//! }
//! ```

use rskit_errors::AppResult;
use serde::Serialize;

use crate::event::Event;
use crate::traits::EventProducer;

/// A facade that simplifies event publishing by automatically constructing [`Event`] envelopes from typed payloads.
///
/// Configured once with a `source` name (typically the service name)
/// and an underlying [`EventProducer`]. Every call to [`publish`](Self::publish)
/// or [`publish_keyed`](Self::publish_keyed) creates a new envelope with a fresh UUID, UTC timestamp,
/// and the pre-configured source.
pub struct EventPublisher<P> {
    producer: P,
    source: String,
}

impl<P: EventProducer> EventPublisher<P> {
    /// Create a new publisher.
    ///
    /// * `producer` – any [`EventProducer`] implementation (Kafka, in-memory, …).
    /// * `source`   – the originating service/component name embedded in every event.
    pub fn new(producer: P, source: impl Into<String>) -> Self {
        Self {
            producer,
            source: source.into(),
        }
    }

    /// Publish a typed payload as a domain event.
    ///
    /// Builds an [`Event`] envelope with:
    /// - a fresh UUID
    /// - the configured `source`
    /// - `event_type` as the event type
    /// - `data` serialized into the payload
    pub async fn publish<T: Serialize + Send + Sync>(
        &self,
        topic: &str,
        event_type: &str,
        data: &T,
    ) -> AppResult<()> {
        let event = Event::new(event_type, &self.source).with_data(data)?;
        self.producer.publish(topic, event).await
    }

    /// Publish a typed payload with an explicit partition key and subject.
    ///
    /// Same as [`publish`](Self::publish) but also sets `Event::subject` to the given `key`,
    /// which messaging adapters typically use as the partition key for ordering guarantees.
    pub async fn publish_keyed<T: Serialize + Send + Sync>(
        &self,
        topic: &str,
        event_type: &str,
        data: &T,
        key: &str,
    ) -> AppResult<()> {
        let event = Event::new(event_type, &self.source)
            .with_subject(key)
            .with_data(data)?;
        self.producer.publish(topic, event).await
    }

    /// Publish a batch of typed payloads as domain events.
    ///
    /// Each item is wrapped in its own [`Event`] envelope.
    pub async fn publish_batch<T: Serialize + Send + Sync>(
        &self,
        topic: &str,
        event_type: &str,
        items: &[T],
    ) -> AppResult<()> {
        let events: Vec<Event> = items
            .iter()
            .map(|item| Event::new(event_type, &self.source).with_data(item))
            .collect::<AppResult<_>>()?;
        self.producer.publish_batch(topic, events).await
    }

    /// Returns a reference to the configured source name.
    pub fn source(&self) -> &str {
        &self.source
    }

    /// Returns a reference to the underlying producer.
    pub const fn producer(&self) -> &P {
        &self.producer
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::EventProducer;
    use async_trait::async_trait;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Serialize, Deserialize, PartialEq)]
    struct TestPayload {
        name: String,
        value: u32,
    }

    use std::sync::Arc;

    use parking_lot::Mutex;

    struct MockEventProducer {
        published: Arc<Mutex<Vec<(String, Event)>>>,
    }

    impl MockEventProducer {
        #[allow(clippy::type_complexity)]
        fn new() -> (Self, Arc<Mutex<Vec<(String, Event)>>>) {
            let published = Arc::new(Mutex::new(Vec::new()));
            (
                Self {
                    published: published.clone(),
                },
                published,
            )
        }
    }

    #[async_trait]
    impl EventProducer for MockEventProducer {
        async fn publish(&self, topic: &str, event: Event) -> AppResult<()> {
            self.published.lock().push((topic.to_string(), event));
            Ok(())
        }

        async fn publish_batch(&self, topic: &str, events: Vec<Event>) -> AppResult<()> {
            {
                let mut guard = self.published.lock();
                for event in events {
                    guard.push((topic.to_string(), event));
                }
            }
            Ok(())
        }
    }

    #[tokio::test]
    async fn publish_creates_envelope() {
        let (mock, sink) = MockEventProducer::new();
        let publisher = EventPublisher::new(mock, "test-service");

        let payload = TestPayload {
            name: "hello".into(),
            value: 42,
        };

        publisher
            .publish("my.topic", "payload.created", &payload)
            .await
            .unwrap();

        let recorded = sink.lock().clone();
        assert_eq!(recorded.len(), 1);
        let (topic, event) = &recorded[0];
        assert_eq!(topic, "my.topic");
        assert_eq!(event.event_type, "payload.created");
        assert_eq!(event.source, "test-service");
        assert!(!event.id.is_empty());
        assert_eq!(event.version, "1.0");

        let parsed: TestPayload = event.parse_data().unwrap();
        assert_eq!(parsed, payload);
    }

    #[tokio::test]
    async fn publish_keyed_sets_subject() {
        let (mock, sink) = MockEventProducer::new();
        let publisher = EventPublisher::new(mock, "order-svc");

        publisher
            .publish_keyed(
                "orders",
                "order.placed",
                &serde_json::json!({"id": "123"}),
                "order-123",
            )
            .await
            .unwrap();

        let recorded = sink.lock().clone();
        let (_, event) = &recorded[0];
        assert_eq!(event.subject, "order-123");
        assert_eq!(event.event_type, "order.placed");
        assert_eq!(event.source, "order-svc");
    }

    #[tokio::test]
    async fn publish_batch_creates_multiple_envelopes() {
        let (mock, sink) = MockEventProducer::new();
        let publisher = EventPublisher::new(mock, "batch-svc");

        let items = vec![
            TestPayload {
                name: "a".into(),
                value: 1,
            },
            TestPayload {
                name: "b".into(),
                value: 2,
            },
            TestPayload {
                name: "c".into(),
                value: 3,
            },
        ];

        publisher
            .publish_batch("items", "item.created", &items)
            .await
            .unwrap();

        let recorded = sink.lock().clone();
        assert_eq!(recorded.len(), 3);
        for (topic, event) in &recorded {
            assert_eq!(topic, "items");
            assert_eq!(event.event_type, "item.created");
            assert_eq!(event.source, "batch-svc");
        }

        // Verify each has unique ID
        let ids: Vec<&str> = recorded.iter().map(|(_, e)| e.id.as_str()).collect();
        let unique: std::collections::HashSet<&str> = ids.iter().copied().collect();
        assert_eq!(unique.len(), 3);
    }

    #[test]
    fn source_accessor() {
        let (mock, _) = MockEventProducer::new();
        let publisher = EventPublisher::new(mock, "my-service");
        assert_eq!(publisher.source(), "my-service");
    }
}
