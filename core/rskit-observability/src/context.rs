//! Operation context for tracking cross-cutting observability concerns.

use std::sync::Arc;
use std::time::Instant;

use tracing::Span;

use crate::metrics::MetricsHandle;
use crate::tracer::set_operation_attributes;

/// Tracks observability context for a single operation.
///
/// Mirrors Go's `observability.OperationContext`.
pub struct OperationContext {
    service_name: String,
    operation_name: String,
    request_id: String,
    user_id: String,
    start_time: Instant,
    metrics: Option<Arc<MetricsHandle>>,
}

impl OperationContext {
    /// Create a new operation context.
    pub fn new(
        service: impl Into<String>,
        operation: impl Into<String>,
        request_id: impl Into<String>,
        user_id: impl Into<String>,
    ) -> Self {
        Self {
            service_name: service.into(),
            operation_name: operation.into(),
            request_id: request_id.into(),
            user_id: user_id.into(),
            start_time: Instant::now(),
            metrics: None,
        }
    }

    /// Attach a metrics handle to this context.
    #[must_use]
    pub fn with_metrics(mut self, metrics: Arc<MetricsHandle>) -> Self {
        self.metrics = Some(metrics);
        self
    }

    /// Name of the service performing the operation.
    #[must_use]
    pub fn service_name(&self) -> &str {
        &self.service_name
    }

    /// Name of the operation being performed.
    #[must_use]
    pub fn operation_name(&self) -> &str {
        &self.operation_name
    }

    /// Unique request identifier for correlation.
    #[must_use]
    pub fn request_id(&self) -> &str {
        &self.request_id
    }

    /// Identifier of the user initiating the request.
    ///
    /// This is retained for caller-side correlation
    /// and is not emitted to spans by default to avoid leaking user identifiers into telemetry backends.
    #[must_use]
    pub fn user_id(&self) -> &str {
        &self.user_id
    }

    /// Create a tracing span for a sub-operation.
    pub fn start_span(&self, name: &str) -> Span {
        let span = tracing::info_span!("operation", otel.name = name);
        set_operation_attributes(
            &span,
            &self.service_name,
            &self.operation_name,
            &self.request_id,
        );
        span
    }

    /// Record the end of the operation with status and optional error.
    pub fn end_operation(&self, status: &str, error: Option<&rskit_errors::AppError>) {
        let duration = self.elapsed();
        tracing::info!(
            service = %self.service_name,
            operation = %self.operation_name,
            request_id = %self.request_id,
            status = status,
            duration_ms = duration.as_millis() as u64,
            error = error.map(|e| e.to_string()).as_deref(),
            "operation completed"
        );
    }

    /// Time elapsed since the context was created.
    pub fn elapsed(&self) -> std::time::Duration {
        self.start_time.elapsed()
    }
}
