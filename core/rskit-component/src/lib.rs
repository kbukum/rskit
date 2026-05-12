//! Component lifecycle primitives shared across rskit crates.

#![warn(missing_docs)]

/// Lifecycle-managed component trait and lazy wrapper.
pub mod component;
/// Health report types.
pub mod health;
/// Ordered component registry with state tracking.
pub mod registry;
/// Component lifecycle states.
pub mod state;

pub use component::{Component, LazyComponent};
pub use health::{Health, HealthStatus};
pub use registry::{Registry, RegistryConfig, StopResult};
pub use state::State;
