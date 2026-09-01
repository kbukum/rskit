//! Generic observe-only hook event types.
//!
//! Domain-specific event types implement [`Event`]. Handlers receive read-only,
//! concrete event references and can only return success or a typed [`HookError`].

use std::any::Any;
use std::fmt;

/// A string-based event type identifier.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EventType(String);

impl EventType {
    /// The canonical event type emitted when a non-fatal hook handler returns an error.
    pub const ON_ERROR: &'static str = "on_error";

    /// Create a new event type from any string-like value.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self(name.into())
    }

    /// The canonical [`EventType`] for non-fatal hook handler errors.
    #[must_use]
    pub fn on_error() -> Self {
        Self::new(Self::ON_ERROR)
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

/// Trait that all hook and in-process bus events must implement.
///
/// The public hook API dispatches on the concrete Rust event type.
/// The [`EventType`] remains available as stable human-readable metadata for logs, metrics,
/// and diagnostics.
pub trait Event: Any + Send + Sync + 'static {
    /// Return the event type discriminator for this event.
    fn event_type(&self) -> EventType;
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

    /// Combine two hook errors into a single aggregate error.
    ///
    /// Messages are joined with `; ` and the result is fatal if either input is fatal.
    /// Used to accumulate every non-fatal error raised during a single emit.
    #[must_use]
    pub fn join(self, other: HookError) -> HookError {
        let message = format!("{}; {}", self.message, other.message);
        HookError {
            message,
            fatal: self.fatal || other.fatal,
        }
    }
}

/// A non-fatal hook handler error, re-emitted as an observation to `on_error` handlers.
///
/// Mirrors the aggregation contract shared with the sibling kits: whenever a handler for some
/// event returns a non-fatal error, the registry emits an [`ErrorEvent`] carrying the failing
/// error and the [`EventType`] that produced it to any handlers registered for
/// [`EventType::on_error`].
#[derive(Debug, Clone)]
pub struct ErrorEvent {
    error: HookError,
    source: EventType,
}

impl ErrorEvent {
    /// Create an error event for `error` produced while dispatching `source`.
    #[must_use]
    pub fn new(error: HookError, source: EventType) -> Self {
        Self { error, source }
    }

    /// The non-fatal error that was raised.
    #[must_use]
    pub fn error(&self) -> &HookError {
        &self.error
    }

    /// The event type that produced the error.
    #[must_use]
    pub fn source(&self) -> &EventType {
        &self.source
    }
}

impl Event for ErrorEvent {
    fn event_type(&self) -> EventType {
        EventType::on_error()
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

#[cfg(test)]
mod tests {
    use super::{ErrorEvent, Event, EventType, HookError, HookResult};

    struct Ping {
        count: u32,
    }

    impl Event for Ping {
        fn event_type(&self) -> EventType {
            EventType::new("ping")
        }
    }

    #[test]
    fn event_type_equality() {
        assert_eq!(EventType::new("ping"), EventType::new("ping"));
        assert_ne!(EventType::new("ping"), EventType::new("pong"));
    }

    #[test]
    fn event_type_is_static_metadata() {
        let ping = Ping { count: 42 };
        assert_eq!(ping.event_type(), EventType::new("ping"));
        assert_eq!(ping.count, 42);
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

    #[test]
    fn hook_error_join_combines_messages_and_fatality() {
        let joined = HookError::new("first").join(HookError::new("second"));
        assert_eq!(joined.message(), "first; second");
        assert!(!joined.is_fatal());

        let with_fatal = HookError::new("warn").join(HookError::fatal("stop"));
        assert!(with_fatal.is_fatal());
        assert_eq!(with_fatal.message(), "warn; stop");
    }

    #[test]
    fn error_event_carries_source_and_error() {
        let event = ErrorEvent::new(HookError::new("boom"), EventType::new("ping"));
        assert_eq!(event.event_type(), EventType::on_error());
        assert_eq!(event.source(), &EventType::new("ping"));
        assert_eq!(event.error().message(), "boom");
    }
}
