//! Generic event hook system.
//!
//! Provides [`HookRegistry`] to register handlers for arbitrary events.
//! Domain-specific event types should implement the [`Event`] trait and live
//! in the consuming crate (e.g. `rskit-agent`).

pub mod registry;
pub mod types;

pub use registry::HookRegistry;
pub use types::{Action, Event, EventType, HookHandler, HookResult};
