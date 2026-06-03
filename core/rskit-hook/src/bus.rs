//! Bounded typed in-process event bus.

use std::marker::PhantomData;
use std::sync::Arc;

use rskit_errors::{AppError, AppResult, ErrorCode};
use tokio::sync::broadcast;

use crate::Event;

/// Configuration for a typed [`EventBus`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EventBusConfig {
    /// Maximum number of events retained for slow subscribers.
    ///
    /// The bus is bounded. When a subscriber falls behind this capacity,
    /// `recv()` returns an error that reports how many events were skipped.
    pub capacity: usize,
}

impl Default for EventBusConfig {
    fn default() -> Self {
        Self { capacity: 1024 }
    }
}

/// A bounded, typed, in-process publish/subscribe bus for event `E`.
///
/// This bus is constructor-injected and process-local. Use `rskit-messaging`
/// for broker-backed cross-process events.
pub struct EventBus<E: Event> {
    sender: broadcast::Sender<Arc<E>>,
}

impl<E: Event> Clone for EventBus<E> {
    fn clone(&self) -> Self {
        Self {
            sender: self.sender.clone(),
        }
    }
}

impl<E: Event> EventBus<E> {
    /// Create a new bounded bus.
    #[must_use]
    pub fn new(config: EventBusConfig) -> Self {
        let capacity = config.capacity.max(1);
        let (sender, _) = broadcast::channel(capacity);
        Self { sender }
    }

    /// Publish an event to all current subscribers.
    ///
    /// Returns the number of subscribers that accepted the event. Publishing to
    /// a bus with no subscribers succeeds and returns `0`.
    pub fn publish(&self, event: E) -> AppResult<usize> {
        self.sender.send(Arc::new(event)).or(Ok(0))
    }

    /// Register a new subscriber.
    #[must_use]
    pub fn subscribe(&self) -> Subscriber<E> {
        Subscriber {
            receiver: self.sender.subscribe(),
        }
    }
}

/// Explicit subscriber registry for a typed event bus.
pub struct EventRegistry<E: Event> {
    bus: EventBus<E>,
}

impl<E: Event> EventRegistry<E> {
    /// Create an empty registry with a bounded bus.
    #[must_use]
    pub fn new(config: EventBusConfig) -> Self {
        Self {
            bus: EventBus::new(config),
        }
    }

    /// Return a publisher handle for the registered bus.
    #[must_use]
    pub fn publisher(&self) -> EventBus<E> {
        self.bus.clone()
    }

    /// Explicitly register a subscriber.
    #[must_use]
    pub fn register_subscriber(&self) -> Subscriber<E> {
        self.bus.subscribe()
    }
}

impl<E: Event> Default for EventRegistry<E> {
    fn default() -> Self {
        Self::new(EventBusConfig::default())
    }
}

/// A typed event subscriber.
pub struct Subscriber<E: Event> {
    receiver: broadcast::Receiver<Arc<E>>,
}

impl<E: Event> Subscriber<E> {
    /// Receive the next event.
    ///
    /// Returns a typed error when the sender is closed or this subscriber lagged
    /// behind the bounded bus capacity.
    pub async fn recv(&mut self) -> AppResult<Arc<E>> {
        self.receiver.recv().await.map_err(|error| match error {
            broadcast::error::RecvError::Closed => {
                AppError::new(ErrorCode::Cancelled, "event bus publisher closed")
            }
            broadcast::error::RecvError::Lagged(skipped) => AppError::new(
                ErrorCode::RateLimited,
                format!("event subscriber lagged by {skipped} messages"),
            ),
        })
    }
}

impl<E: Event> std::fmt::Debug for EventBus<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EventBus")
            .field("event_type", &std::any::type_name::<E>())
            .field("receiver_count", &self.sender.receiver_count())
            .finish()
    }
}

impl<E: Event> std::fmt::Debug for EventRegistry<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EventRegistry")
            .field("event_type", &std::any::type_name::<E>())
            .finish_non_exhaustive()
    }
}

impl<E: Event> std::fmt::Debug for Subscriber<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let _phantom = PhantomData::<E>;
        f.debug_struct("Subscriber")
            .field("event_type", &std::any::type_name::<E>())
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::{EventBus, EventBusConfig, EventRegistry};
    use crate::{Event, EventType};

    #[derive(Debug)]
    struct Ping(u32);

    impl Event for Ping {
        fn event_type(&self) -> EventType {
            EventType::new("ping")
        }
    }

    #[tokio::test]
    async fn bus_publishes_to_registered_subscriber() {
        let bus = EventBus::<Ping>::new(EventBusConfig { capacity: 4 });
        let mut subscriber = bus.subscribe();

        assert_eq!(bus.publish(Ping(7)).expect("publish"), 1);
        let event = subscriber.recv().await.expect("receive");
        assert_eq!(event.0, 7);
    }

    #[tokio::test]
    async fn publish_without_subscribers_succeeds() {
        let bus = EventBus::<Ping>::new(EventBusConfig { capacity: 4 });
        assert_eq!(bus.publish(Ping(1)).expect("publish"), 0);
    }

    #[tokio::test]
    async fn subscriber_reports_lag() {
        let bus = EventBus::<Ping>::new(EventBusConfig { capacity: 1 });
        let mut subscriber = bus.subscribe();

        let _ = bus.publish(Ping(1)).expect("first");
        let _ = bus.publish(Ping(2)).expect("second");
        let err = subscriber.recv().await.expect_err("lag should be reported");
        assert_eq!(err.code(), rskit_errors::ErrorCode::RateLimited);
    }

    #[test]
    fn event_registry_registers_subscribers_explicitly() {
        let registry = EventRegistry::<Ping>::new(EventBusConfig { capacity: 4 });
        let _subscriber = registry.register_subscriber();
        let publisher = registry.publisher();
        assert_eq!(publisher.publish(Ping(1)).expect("publish"), 1);
    }
}
