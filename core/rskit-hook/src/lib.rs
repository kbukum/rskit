//! Typed in-process hooks and event bus primitives.
//!
//! Domain-specific event payloads implement [`Event`] and are registered with
//! [`HookRegistry::on`] using the concrete event type. This keeps hook payloads
//! typed at the public API boundary while allowing a single injected registry
//! to coordinate multiple event types.

#![warn(missing_docs)]

/// Bounded typed in-process event bus.
pub mod bus;
/// Typed hook registry.
pub mod registry;
/// Event and hook result types.
pub mod types;

pub use bus::{EventBus, EventBusConfig, EventRegistry, Subscriber};
pub use registry::HookRegistry as Registry;
pub use registry::{HookRegistry, LifecycleHookRegistry};
pub use tokio_util::sync::CancellationToken;
pub use types::{Event, EventType, HookError, HookResult};
