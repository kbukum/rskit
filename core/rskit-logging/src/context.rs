//! Span-level context helpers — component tagging, request enrichment.

/// Returns a new [`tracing::Span`] with a `component` field set.
///
/// # Example
///
/// ```rust
/// # use rskit_logging::context::component_span;
/// let _span = component_span("auth-service").entered();
/// tracing::info!("component log");
/// ```
pub fn component_span(name: &'static str) -> tracing::Span {
    tracing::info_span!("component", component = name)
}

/// Returns a new [`tracing::Span`] enriched with HTTP request metadata.
pub fn request_span(method: &str, path: &str, request_id: &str) -> tracing::Span {
    tracing::info_span!(
        "request",
        http.method  = %method,
        http.path    = %path,
        request_id   = %request_id,
    )
}

/// Injects a correlation ID into the current span.
pub fn set_correlation_id(id: &str) {
    tracing::Span::current().record("correlation_id", id);
}

/// Injects a user ID into the current span.
pub fn set_user_id(id: &str) {
    tracing::Span::current().record("user_id", id);
}

/// Injects a trace ID into the current span.
pub fn set_trace_id(id: &str) {
    tracing::Span::current().record("trace_id", id);
}
