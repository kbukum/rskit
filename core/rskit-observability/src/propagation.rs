//! W3C Trace-Context propagation helpers.
//!
//! Public HTTP propagation API; implementation lives in the `tracer` module.

/// Inject the current span's trace context into HTTP headers.
///
/// Uses the W3C `traceparent` / `tracestate` headers
/// so that downstream services can join the same distributed trace.
pub fn inject_trace_context(headers: &mut http::HeaderMap) {
    super::tracer::inject_trace_context(headers);
}

/// Extract a trace context from incoming HTTP headers.
///
/// Call this on the server side and attach the returned context to the processing span
/// so it becomes a child of the upstream caller.
pub fn extract_trace_context(headers: &http::HeaderMap) -> opentelemetry::Context {
    super::tracer::extract_trace_context(headers)
}
