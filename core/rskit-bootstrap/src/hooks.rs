//! Bootstrap lifecycle hook events.

use std::any::Any;

use rskit_hook::{Event, EventType};

/// Emitted after components start and before readiness checks.
#[derive(Debug, Clone)]
pub struct AppStarted {
    pub runtime_handle: tokio::runtime::Handle,
}

impl Event for AppStarted {
    fn event_type(&self) -> EventType {
        EventType::new("bootstrap:start")
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// Emitted after readiness checks and before the app is marked ready.
#[derive(Debug, Clone)]
pub struct AppReady {
    pub runtime_handle: tokio::runtime::Handle,
}

impl Event for AppReady {
    fn event_type(&self) -> EventType {
        EventType::new("bootstrap:ready")
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// Emitted during shutdown before components are stopped.
#[derive(Debug, Clone)]
pub struct AppStopping {
    pub runtime_handle: tokio::runtime::Handle,
}

impl Event for AppStopping {
    fn event_type(&self) -> EventType {
        EventType::new("bootstrap:stop")
    }
    fn as_any(&self) -> &dyn Any {
        self
    }
}

/// Lifecycle phases emitted by bootstrap.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum LifecyclePhase {
    Start,
    Ready,
    Stop,
}
