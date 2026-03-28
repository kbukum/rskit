//! OpenTelemetry tracing, metrics, and context propagation.
//!
//! This crate wires up the OpenTelemetry SDK with the `tracing` ecosystem
//! so that spans and metrics are automatically exported via OTLP.

#![warn(missing_docs)]

/// OpenTelemetry metrics pipeline.
pub mod metrics;
/// W3C Trace-Context propagation helpers.
pub mod propagation;
/// OpenTelemetry tracer initialization.
pub mod tracer;

pub use metrics::{MetricsConfig, MetricsHandle, init_metrics};
pub use propagation::{extract_trace_context, inject_trace_context};
pub use tracer::{TracerGuard, TracingConfig, init_tracer};
