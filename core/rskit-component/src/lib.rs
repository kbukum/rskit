//! Component lifecycle primitives shared across rskit crates.

#![warn(missing_docs)]

/// Lifecycle-managed component trait and lazy wrapper.
pub mod component;
/// Health report types.
pub mod health;
/// Optional component self-description metadata capabilities.
pub mod metadata;
/// Ordered component registry with state tracking.
pub mod registry;
/// Component registry configuration.
pub mod registry_config;
/// Component lifecycle states.
pub mod state;
/// Component shutdown result types.
pub mod stop;

pub use component::{Component, LazyComponent};
pub use health::{Health, HealthStatus};
pub use metadata::{Describable, Description, Route, RouteProvider};
pub use registry::Registry;
pub use registry_config::RegistryConfig;
pub use state::State;
pub use stop::StopResult;
