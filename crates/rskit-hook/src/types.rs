//! Generic hook types: event trait, actions, results, and handler definitions.
//!
//! This module provides domain-agnostic primitives. Domain-specific event types
//! (e.g. tool calls, LLM calls) should be defined in the consuming crate and
//! implement the [`Event`] trait.

use std::any::Any;
use std::fmt;

use tokio_util::sync::CancellationToken;

/// A string-based event type identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EventType(String);

impl EventType {
    /// Create a new event type from any string-like value.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }

    /// Return the inner string slice.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for EventType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Trait that all hook events must implement.
pub trait Event: Any + Send + Sync {
    /// The event type discriminator for this event.
    fn event_type(&self) -> EventType;

    /// Upcast to `&dyn Any` for downcasting in handlers.
    fn as_any(&self) -> &dyn Any;
}

/// What the pipeline should do after processing a hook handler.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Action {
    /// Continue normal execution.
    Continue,
    /// Abort the current operation.
    Abort,
    /// The handler has modified data (check `modified_data`).
    Modify,
}

/// The outcome returned by a hook handler.
pub struct HookResult {
    /// The action the pipeline should take.
    pub action: Action,
    /// Optional modified payload for `Action::Modify`.
    pub modified_data: Option<Box<dyn Any + Send>>,
    /// Human-readable explanation.
    pub reason: String,
    /// Optional error surfaced by the handler.
    pub error: Option<Box<dyn std::error::Error + Send + Sync>>,
}

impl fmt::Debug for HookResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HookResult")
            .field("action", &self.action)
            .field("has_modified_data", &self.modified_data.is_some())
            .field("reason", &self.reason)
            .field("has_error", &self.error.is_some())
            .finish()
    }
}

impl HookResult {
    /// Convenience: continue with no modifications.
    #[must_use]
    pub fn ok() -> Self {
        Self {
            action: Action::Continue,
            modified_data: None,
            reason: String::new(),
            error: None,
        }
    }

    /// Convenience: continue while attaching an error.
    #[must_use]
    pub fn continue_with_error(error: impl std::error::Error + Send + Sync + 'static) -> Self {
        let reason = error.to_string();
        Self {
            action: Action::Continue,
            modified_data: None,
            reason,
            error: Some(Box::new(error)),
        }
    }

    /// Convenience: abort with a reason.
    #[must_use]
    pub fn abort(reason: impl Into<String>) -> Self {
        Self {
            action: Action::Abort,
            modified_data: None,
            reason: reason.into(),
            error: None,
        }
    }

    /// Convenience: abort while attaching an error.
    #[must_use]
    pub fn abort_with_error(error: impl std::error::Error + Send + Sync + 'static) -> Self {
        let reason = error.to_string();
        Self {
            action: Action::Abort,
            modified_data: None,
            reason,
            error: Some(Box::new(error)),
        }
    }

    /// Convenience: modify with typed data.
    #[must_use]
    pub fn modify(data: impl Any + Send + 'static, reason: impl Into<String>) -> Self {
        Self {
            action: Action::Modify,
            modified_data: Some(Box::new(data)),
            reason: reason.into(),
            error: None,
        }
    }
}

impl Default for HookResult {
    fn default() -> Self {
        Self::ok()
    }
}

/// A boxed function that handles a hook event.
pub type HookHandler = Box<dyn Fn(CancellationToken, &dyn Event) -> HookResult + Send + Sync>;

#[cfg(test)]
mod tests {
    use std::io;

    use super::{Action, Event, EventType, HookResult};

    struct Ping {
        count: u32,
    }

    impl Event for Ping {
        fn event_type(&self) -> EventType {
            EventType::new("ping")
        }

        fn as_any(&self) -> &dyn std::any::Any {
            self
        }
    }

    #[test]
    fn test_event_type_equality() {
        assert_eq!(EventType::new("ping"), EventType::new("ping"));
        assert_ne!(EventType::new("ping"), EventType::new("pong"));
    }

    #[test]
    fn test_event_trait() {
        let ping = Ping { count: 42 };
        let any = ping.as_any();
        let downcasted = any
            .downcast_ref::<Ping>()
            .expect("ping downcast should succeed");
        assert_eq!(downcasted.count, 42);
    }

    #[test]
    fn test_hook_result_ok() {
        let result = HookResult::ok();
        assert_eq!(result.action, Action::Continue);
        assert!(result.error.is_none());
    }

    #[test]
    fn test_hook_result_continue_with_error() {
        let result = HookResult::continue_with_error(io::Error::other("warn"));
        assert_eq!(result.action, Action::Continue);
        assert_eq!(result.reason, "warn");
        assert!(result.error.is_some());
    }

    #[test]
    fn test_hook_result_abort_with_error() {
        let result = HookResult::abort_with_error(io::Error::other("blocked"));
        assert_eq!(result.action, Action::Abort);
        assert_eq!(result.reason, "blocked");
        assert!(result.error.is_some());
    }

    #[test]
    fn test_hook_result_modify() {
        let result = HookResult::modify(42_u32, "changed value");
        let value = result
            .modified_data
            .expect("modified result should include data");
        assert_eq!(value.downcast_ref::<u32>(), Some(&42));
    }

    #[test]
    fn test_hook_result_debug() {
        let debug = format!(
            "{:?}",
            HookResult::continue_with_error(io::Error::other("warn"))
        );
        assert!(debug.contains("has_error"));
    }
}
