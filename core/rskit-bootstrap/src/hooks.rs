//! Bootstrap lifecycle hook events.

use std::any::Any;

use rskit_hook::{Event, EventType};

/// Lifecycle events emitted by [`crate::App`] during startup and shutdown.
#[derive(Debug, Clone)]
pub struct LifecycleEvent {
    kind: LifecycleEventType,
    runtime_handle: tokio::runtime::Handle,
}

impl LifecycleEvent {
    /// Create a lifecycle event for the given kind and runtime.
    #[must_use]
    pub fn new(kind: LifecycleEventType, runtime_handle: tokio::runtime::Handle) -> Self {
        Self {
            kind,
            runtime_handle,
        }
    }

    /// Return the lifecycle event kind.
    #[must_use]
    pub fn kind(&self) -> LifecycleEventType {
        self.kind
    }

    /// Return the Tokio runtime handle used to drive async lifecycle handlers.
    #[must_use]
    pub fn runtime_handle(&self) -> &tokio::runtime::Handle {
        &self.runtime_handle
    }
}

impl Event for LifecycleEvent {
    fn event_type(&self) -> EventType {
        self.kind.event_type()
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// Lifecycle phases emitted by bootstrap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum LifecycleEventType {
    /// Emitted after components start and before readiness checks.
    EventStart,
    /// Emitted after readiness checks and before the app is marked ready.
    EventReady,
    /// Emitted during shutdown before components are stopped.
    EventStop,
}

impl LifecycleEventType {
    /// Return the hook event type for this lifecycle phase.
    #[must_use]
    pub fn event_type(self) -> EventType {
        match self {
            Self::EventStart => EventType::new("bootstrap:start"),
            Self::EventReady => EventType::new("bootstrap:ready"),
            Self::EventStop => EventType::new("bootstrap:stop"),
        }
    }
}
