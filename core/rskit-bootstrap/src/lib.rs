//! Application lifecycle orchestration: typestate `App` and startup summary.

#![warn(missing_docs)]

/// Typestate [`App`] and [`AppBuilder`].
pub mod app;
/// Lifecycle hook events.
pub mod hooks;
mod lifecycle;
mod summary;

pub use app::{App, AppBuilder, Built, Started, Stopped, Unconfigured};
pub use hooks::{LifecycleEvent, LifecycleEventType};
pub use rskit_component::component;
pub use rskit_component::health;
pub use rskit_component::registry;
pub use rskit_component::state;
pub use rskit_component::{
    Component, Health, HealthStatus, LazyComponent, Registry, RegistryConfig, State, StopResult,
};

// Re-export CancellationToken so downstream crates don't need tokio-util directly
pub use tokio_util::sync::CancellationToken;
