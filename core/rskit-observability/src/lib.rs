//! OpenTelemetry tracing, metrics, logging, and context propagation.
//!
//! This crate wires up the OpenTelemetry SDK with the `tracing` ecosystem
//! so that spans, metrics, and logs are automatically exported via OTLP.

#![warn(missing_docs)]

/// Operation context for cross-cutting observability concerns.
pub mod context;
/// Service health tracking for aggregate component monitoring.
pub mod health;
/// Structured logging setup using the `tracing` ecosystem.
pub mod logging;
/// OpenTelemetry metrics pipeline.
pub mod metrics;
/// W3C Trace-Context propagation helpers.
pub mod propagation;
/// OpenTelemetry tracer initialization.
pub mod tracer;

pub use context::OperationContext;
pub use health::{ComponentHealth, HealthStatus, ServiceHealth};
pub use logging::*;
pub use metrics::{MetricsConfig, MetricsHandle, init_metrics};
pub use propagation::{extract_trace_context, inject_trace_context};
pub use tracer::{TracerGuard, TracingConfig, init_tracer};
