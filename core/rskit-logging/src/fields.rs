//! Standard field names for the unified log schema.
//!
//! These constants ensure consistency across all kits (gokit, rskit, pykit) when emitting structured log fields.

/// Standard field names for the unified log schema.
pub mod names {
    /// The service name emitting the log entry.
    pub const SERVICE: &str = "service";
    /// Deployment environment (development, staging, production).
    pub const ENVIRONMENT: &str = "environment";
    /// Logical component within the service (e.g. "auth", "cache").
    pub const COMPONENT: &str = "component";
    /// Distributed trace identifier.
    pub const TRACE_ID: &str = "trace_id";
    /// Span identifier within a trace.
    pub const SPAN_ID: &str = "span_id";
    /// Correlation identifier for cross-service request tracking.
    pub const CORRELATION_ID: &str = "correlation_id";
    /// User identifier.
    pub const USER_ID: &str = "user_id";
    /// HTTP request identifier.
    pub const REQUEST_ID: &str = "request_id";
    /// Duration of an operation in milliseconds.
    pub const DURATION_MS: &str = "duration_ms";
    /// Service version string.
    pub const VERSION: &str = "version";
}

#[cfg(test)]
mod tests {
    use super::names::*;

    #[test]
    fn field_constants_are_non_empty() {
        let fields = [
            SERVICE,
            ENVIRONMENT,
            COMPONENT,
            TRACE_ID,
            SPAN_ID,
            CORRELATION_ID,
            USER_ID,
            REQUEST_ID,
            DURATION_MS,
            VERSION,
        ];
        for f in fields {
            assert!(!f.is_empty(), "field constant must not be empty");
        }
    }

    #[test]
    fn field_constants_are_snake_case() {
        let fields = [
            SERVICE,
            ENVIRONMENT,
            COMPONENT,
            TRACE_ID,
            SPAN_ID,
            CORRELATION_ID,
            USER_ID,
            REQUEST_ID,
            DURATION_MS,
            VERSION,
        ];
        for f in fields {
            assert!(
                f.chars().all(|c| c.is_ascii_lowercase() || c == '_'),
                "field `{f}` must be snake_case"
            );
        }
    }
}
