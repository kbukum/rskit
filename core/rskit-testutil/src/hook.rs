//! Hook test helpers.

use rskit_hook::{Event, EventType};

/// Simple typed event for hook and event-bus tests.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TestEvent {
    /// Stable event type name.
    pub event_type: EventType,
    /// Human-readable payload.
    pub message: String,
}

impl TestEvent {
    /// Create a test event.
    #[must_use]
    pub fn new(event_type: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            event_type: EventType::new(event_type),
            message: message.into(),
        }
    }
}

impl Event for TestEvent {
    fn event_type(&self) -> EventType {
        self.event_type.clone()
    }
}
