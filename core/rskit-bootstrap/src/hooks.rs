//! Bootstrap lifecycle phase metadata.

use rskit_hook::{Event, EventType};

/// Lifecycle phases emitted by bootstrap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum LifecycleEventType {
    /// Emitted before components start.
    BeforeStart,
    /// Emitted after components start and readiness checks pass.
    AfterStart,
    /// Emitted before components stop.
    BeforeStop,
    /// Emitted after components stop.
    AfterStop,
}

impl LifecycleEventType {
    /// Return the stable lifecycle phase label.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::BeforeStart => "bootstrap:before_start",
            Self::AfterStart => "bootstrap:after_start",
            Self::BeforeStop => "bootstrap:before_stop",
            Self::AfterStop => "bootstrap:after_stop",
        }
    }
}

/// Lifecycle event metadata for observers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LifecycleEvent {
    kind: LifecycleEventType,
}

impl LifecycleEvent {
    /// Create a lifecycle event for the given phase.
    #[must_use]
    pub const fn new(kind: LifecycleEventType) -> Self {
        Self { kind }
    }

    /// Return the lifecycle phase.
    #[must_use]
    pub const fn kind(&self) -> LifecycleEventType {
        self.kind
    }
}

impl Event for LifecycleEvent {
    fn event_type(&self) -> EventType {
        EventType::new(self.kind.as_str())
    }
}
