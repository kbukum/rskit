//! OpenTelemetry tracing, metrics, and context propagation.
//!
//! This crate wires up the OpenTelemetry SDK with the `tracing` ecosystem
//! so that spans and metrics are automatically exported via OTLP.

#![warn(missing_docs)]

/// Operation context for cross-cutting observability concerns.
pub mod context;
/// Service health tracking for aggregate component monitoring.
pub mod health;
/// OpenTelemetry log pipeline.
pub mod logs;
/// OpenTelemetry metrics pipeline.
pub mod metrics;
/// W3C Trace-Context propagation helpers.
pub mod propagation;
/// OpenTelemetry tracer initialization.
mod tracer;

pub use context::OperationContext;
pub use health::{ComponentHealth, HealthStatus, ServiceHealth};
pub use logs::{LogsConfig, LogsHandle, init_logs};
pub use metrics::{MetricsConfig, MetricsHandle, init_metrics, init_metrics_with_protocol};
pub use propagation::{extract_trace_context, inject_trace_context};
pub use tracer::{
    OtlpProtocol, SERVICE_NAME, TracerGuard, TracingConfig, init_tracer, set_operation_attributes,
    tracer_provider, tracer_provider_with_protocol,
};
