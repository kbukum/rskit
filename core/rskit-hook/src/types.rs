//! Generic observe-only hook types.
//!
//! Domain-specific event types implement [`Event`]. Handlers receive read-only
//! event references and can only return success or a typed [`HookError`].

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

/// Hook failure. Fatal errors are rare and may stop the owning loop.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HookError {
    message: String,
    fatal: bool,
}

impl HookError {
    /// Create a non-fatal hook error.
    #[must_use]
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            fatal: false,
        }
    }

    /// Create a fatal hook error.
    #[must_use]
    pub fn fatal(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            fatal: true,
        }
    }

    /// Whether the owning loop should stop.
    #[must_use]
    pub const fn is_fatal(&self) -> bool {
        self.fatal
    }

    /// Error message.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }
}

impl fmt::Display for HookError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for HookError {}

/// The outcome returned by a hook handler.
pub type HookResult = Result<(), HookError>;

/// A boxed function that handles a hook event.
pub type HookHandler = Box<dyn Fn(CancellationToken, &dyn Event) -> HookResult + Send + Sync>;

#[cfg(test)]
mod tests {
    use super::{Event, EventType, HookError, HookResult};

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
    fn event_type_equality() {
        assert_eq!(EventType::new("ping"), EventType::new("ping"));
        assert_ne!(EventType::new("ping"), EventType::new("pong"));
    }

    #[test]
    fn event_trait_downcasts() {
        let ping = Ping { count: 42 };
        let downcasted = ping.as_any().downcast_ref::<Ping>().expect("ping downcast");
        assert_eq!(downcasted.count, 42);
    }

    #[test]
    fn hook_result_success() {
        let result: HookResult = Ok(());
        assert!(result.is_ok());
    }

    #[test]
    fn hook_error_fatal_flag() {
        let err = HookError::fatal("budget exceeded");
        assert!(err.is_fatal());
        assert_eq!(err.message(), "budget exceeded");
    }
}
