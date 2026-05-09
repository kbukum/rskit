//! Generic event hook system.
//!
//! Provides [`Registry`] to register handlers for arbitrary events.
//! Domain-specific event types should implement the [`Event`] trait and live
//! in the consuming crate (e.g. `rskit-agent`).

pub mod registry;
pub mod types;

pub use registry::HookRegistry;
pub use registry::HookRegistry as Registry;
pub use tokio_util::sync::CancellationToken;
pub use types::{Event, EventType, HookError, HookHandler, HookResult};
