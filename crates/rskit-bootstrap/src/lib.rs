//! Application lifecycle orchestration: typestate `App`, component registry, and lifecycle hooks.

#![warn(missing_docs)]

/// Typestate [`App`] and [`AppBuilder`].
pub mod app;
/// [`Component`] trait for lifecycle-managed infrastructure.
pub mod component;
/// [`Health`] and [`HealthStatus`] types.
pub mod health;
/// Ordered component [`Registry`].
pub mod registry;
/// Startup summary printer.
pub mod summary;

pub use app::{App, AppBuilder, Unconfigured};
pub use component::Component;
pub use health::{Health, HealthStatus};
pub use registry::Registry;

// Re-export CancellationToken so downstream crates don't need tokio-util directly
pub use tokio_util::sync::CancellationToken;
