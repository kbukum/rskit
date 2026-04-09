//! Event hook system for agentic pipelines.
//!
//! Provides [`HookRegistry`] to register handlers for lifecycle events such as
//! pre/post tool calls, pre/post LLM calls, errors, and turn boundaries.

pub mod registry;
pub mod types;

pub use registry::HookRegistry;
pub use types::{Action, EventType, HookEvent, HookHandler, HookResult};
