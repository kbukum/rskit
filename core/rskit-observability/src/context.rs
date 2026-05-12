//! Operation context for tracking cross-cutting observability concerns.

use std::sync::Arc;
use std::time::Instant;

use tracing::Span;

use crate::metrics::MetricsHandle;

/// Tracks observability context for a single operation.
///
/// Mirrors Go's `observability.OperationContext`.
pub struct OperationContext {
    /// Name of the service performing the operation.
    pub service_name: String,
    /// Name of the operation being performed.
    pub operation_name: String,
    /// Unique request identifier for correlation.
    pub request_id: String,
    /// Identifier of the user initiating the request.
    pub user_id: String,
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

    /// Create a tracing span for a sub-operation.
    pub fn start_span(&self, name: &str) -> Span {
        tracing::info_span!(
            "operation",
            "otel.name" = name,
            "service.name" = %self.service_name,
            "rpc.method" = %self.operation_name,
            "http.request_id" = %self.request_id,
            "user.id" = %self.user_id,
        )
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
