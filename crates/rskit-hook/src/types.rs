//! Generic hook types: event trait, actions, results, and handler definitions.
//!
//! This module provides domain-agnostic primitives. Domain-specific event types
//! (e.g. tool calls, LLM calls) should be defined in the consuming crate and
//! implement the [`Event`] trait.

use std::any::Any;
use std::fmt;

// ── EventType ───────────────────────────────────────────────────────────────

/// A string-based event type identifier.
///
/// Using a newtype over `String` rather than a fixed enum keeps the hook module
/// free of domain knowledge — callers define their own event type constants.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EventType(String);

impl EventType {
    /// Create a new event type from any string-like value.
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }

    /// Return the inner string slice.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for EventType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

// ── Event trait ─────────────────────────────────────────────────────────────

/// Trait that all hook events must implement.
///
/// Provides a discriminator via [`Event::event_type`] and allows downcasting
/// through [`Any`] so handlers can inspect the concrete type when needed.
pub trait Event: Any + Send + Sync {
    /// The event type discriminator for this event.
    fn event_type(&self) -> EventType;

    /// Upcast to `&dyn Any` for downcasting in handlers.
    fn as_any(&self) -> &dyn Any;
}

// ── Action / HookResult ────────────────────────────────────────────────────

/// What the pipeline should do after processing a hook handler.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
}

impl fmt::Debug for HookResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("HookResult")
            .field("action", &self.action)
            .field("has_modified_data", &self.modified_data.is_some())
            .field("reason", &self.reason)
            .finish()
    }
}

impl HookResult {
    /// Convenience: continue with no modifications.
    pub fn ok() -> Self {
        Self {
            action: Action::Continue,
            modified_data: None,
            reason: String::new(),
        }
    }

    /// Convenience: abort with a reason.
    pub fn abort(reason: impl Into<String>) -> Self {
        Self {
            action: Action::Abort,
            modified_data: None,
            reason: reason.into(),
        }
    }

    /// Convenience: modify with typed data.
    pub fn modify(data: impl Any + Send + 'static, reason: impl Into<String>) -> Self {
        Self {
            action: Action::Modify,
            modified_data: Some(Box::new(data)),
            reason: reason.into(),
        }
    }
}

impl Default for HookResult {
    fn default() -> Self {
        Self::ok()
    }
}

/// A boxed function that handles a hook event.
pub type HookHandler = Box<dyn Fn(&dyn Event) -> HookResult + Send + Sync>;

#[cfg(test)]
mod tests {
    use super::*;

    // ── Test-local event types ──────────────────────────────────────────

    struct Ping {
        count: u32,
    }

    impl Event for Ping {
        fn event_type(&self) -> EventType {
            EventType::new("ping")
        }
        fn as_any(&self) -> &dyn Any {
            self
        }
    }

    // ── Tests ───────────────────────────────────────────────────────────

    #[test]
    fn test_event_type_equality() {
        let a = EventType::new("ping");
        let b = EventType::new("ping");
        let c = EventType::new("pong");
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn test_event_type_display() {
        let et = EventType::new("pre_tool_call");
        assert_eq!(et.to_string(), "pre_tool_call");
        assert_eq!(et.as_str(), "pre_tool_call");
    }

    #[test]
    fn test_event_trait() {
        let ping = Ping { count: 42 };
        assert_eq!(ping.event_type(), EventType::new("ping"));

        let any = ping.as_any();
        let downcasted = any.downcast_ref::<Ping>().unwrap();
        assert_eq!(downcasted.count, 42);
    }

    #[test]
    fn test_hook_result_ok() {
        let r = HookResult::ok();
        assert_eq!(r.action, Action::Continue);
        assert!(r.modified_data.is_none());
        assert!(r.reason.is_empty());
    }

    #[test]
    fn test_hook_result_abort() {
        let r = HookResult::abort("safety check failed");
        assert_eq!(r.action, Action::Abort);
        assert_eq!(r.reason, "safety check failed");
    }

    #[test]
    fn test_hook_result_modify() {
        let r = HookResult::modify(42_u32, "changed value");
        assert_eq!(r.action, Action::Modify);
        assert_eq!(r.reason, "changed value");
        let val = r.modified_data.unwrap();
        assert_eq!(*val.downcast_ref::<u32>().unwrap(), 42);
    }

    #[test]
    fn test_hook_result_default() {
        let r = HookResult::default();
        assert_eq!(r.action, Action::Continue);
    }

    #[test]
    fn test_action_equality() {
        assert_eq!(Action::Continue, Action::Continue);
        assert_ne!(Action::Continue, Action::Abort);
        assert_ne!(Action::Abort, Action::Modify);
    }

    #[test]
    fn test_hook_result_debug() {
        let r = HookResult::ok();
        let dbg = format!("{r:?}");
        assert!(dbg.contains("HookResult"));
    }
}
