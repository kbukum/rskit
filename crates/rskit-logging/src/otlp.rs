//! OpenTelemetry Logs bridge with OTLP export.
//!
//! Bridges [`tracing`] events to the OpenTelemetry Logs SDK so they can be
//! exported via OTLP (gRPC or HTTP) to a collector.
//!
//! # Feature gate
//!
//! This module is only available when the `otlp` cargo feature is enabled.
//!
//! # Example
//!
//! ```rust,ignore
//! use rskit_logging::otlp::{OtlpConfig, OtlpProvider};
//!
//! let cfg = OtlpConfig { enabled: true, ..Default::default() };
//! let provider = OtlpProvider::new(&cfg, "my-svc", "production", "1.0.0")?;
//! // provider is Some when enabled — add its layer to the subscriber stack.
//! ```

use std::collections::HashMap;

use opentelemetry::KeyValue;
use opentelemetry_appender_tracing::layer::OpenTelemetryTracingBridge;
use opentelemetry_otlp::WithExportConfig;
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::logs::{SdkLogger, SdkLoggerProvider};
use tracing::Subscriber;
use tracing_subscriber::registry::LookupSpan;

// ── Config ──────────────────────────────────────────────────────────────────

/// Configuration for OTLP log export.
#[derive(Debug, Clone)]
pub struct OtlpConfig {
    /// Master switch — when `false`, [`OtlpProvider::new`] returns `Ok(None)`.
    pub enabled: bool,
    /// Collector endpoint (default: `"http://localhost:4317"`).
    pub endpoint: String,
    /// Protocol: `"grpc"` or `"http"` (default: `"grpc"`).
    pub protocol: String,
    /// Skip TLS verification (for development).
    pub insecure: bool,
    /// Additional headers (e.g. auth tokens).
    pub headers: HashMap<String, String>,
}

impl Default for OtlpConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            endpoint: "http://localhost:4317".to_string(),
            protocol: "grpc".to_string(),
            insecure: false,
            headers: HashMap::new(),
        }
    }
}

// ── Provider ────────────────────────────────────────────────────────────────

/// Manages the OpenTelemetry [`SdkLoggerProvider`] for OTLP export.
///
/// Create via [`OtlpProvider::new`], then call [`OtlpProvider::layer`] to
/// obtain a [`tracing_subscriber::Layer`] that can be composed into the
/// subscriber stack.
///
/// The provider **must** be shut down gracefully via [`OtlpProvider::shutdown`]
/// (or by dropping the [`crate::LoggingGuard`]) to flush pending log records.
pub struct OtlpProvider {
    provider: SdkLoggerProvider,
}

impl OtlpProvider {
    /// Create a new OTLP provider.
    ///
    /// Returns `Ok(None)` when `cfg.enabled` is `false`.
    pub fn new(
        cfg: &OtlpConfig,
        service_name: &str,
        environment: &str,
        version: &str,
    ) -> Result<Option<Self>, Box<dyn std::error::Error + Send + Sync>> {
        if !cfg.enabled {
            return Ok(None);
        }

        let resource = Resource::builder_empty()
            .with_attributes([
                KeyValue::new("service.name", service_name.to_string()),
                KeyValue::new("deployment.environment", environment.to_string()),
                KeyValue::new("service.version", version.to_string()),
            ])
            .build();

        let exporter = build_exporter(cfg)?;

        let provider = SdkLoggerProvider::builder()
            .with_resource(resource)
            .with_batch_exporter(exporter)
            .build();

        Ok(Some(Self { provider }))
    }

    /// Get the OpenTelemetry tracing layer for use with
    /// [`tracing_subscriber`].
    ///
    /// The returned layer converts every [`tracing`] event into an
    /// OpenTelemetry log record and forwards it to the OTLP exporter.
    pub fn layer<S>(&self) -> OpenTelemetryTracingBridge<SdkLoggerProvider, SdkLogger>
    where
        S: Subscriber + for<'a> LookupSpan<'a>,
    {
        OpenTelemetryTracingBridge::new(&self.provider)
    }

    /// Gracefully shut down the provider, flushing all pending log records.
    pub fn shutdown(self) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
        self.provider.shutdown().map_err(|e| {
            let boxed: Box<dyn std::error::Error + Send + Sync> = Box::new(e);
            boxed
        })
    }
}

// ── Internal helpers ────────────────────────────────────────────────────────

/// Build an OTLP [`LogExporter`](opentelemetry_otlp::LogExporter) from config.
fn build_exporter(
    cfg: &OtlpConfig,
) -> Result<opentelemetry_otlp::LogExporter, Box<dyn std::error::Error + Send + Sync>> {
    match cfg.protocol.as_str() {
        "http" => {
            let exporter = opentelemetry_otlp::LogExporter::builder()
                .with_http()
                .with_endpoint(&cfg.endpoint)
                .build()?;
            Ok(exporter)
        }
        // Default to gRPC.
        _ => {
            let exporter = opentelemetry_otlp::LogExporter::builder()
                .with_tonic()
                .with_endpoint(&cfg.endpoint)
                .build()?;
            Ok(exporter)
        }
    }
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_is_disabled() {
        let cfg = OtlpConfig::default();
        assert!(!cfg.enabled);
        assert_eq!(cfg.endpoint, "http://localhost:4317");
        assert_eq!(cfg.protocol, "grpc");
        assert!(!cfg.insecure);
        assert!(cfg.headers.is_empty());
    }

    #[test]
    fn disabled_config_returns_none() {
        let cfg = OtlpConfig::default();
        let result = OtlpProvider::new(&cfg, "test-svc", "test", "0.1.0");
        assert!(result.is_ok());
        assert!(result.unwrap().is_none());
    }

    #[test]
    fn config_clone_preserves_values() {
        let mut cfg = OtlpConfig::default();
        cfg.enabled = true;
        cfg.endpoint = "http://collector:4317".to_string();
        cfg.headers
            .insert("x-api-key".to_string(), "secret".to_string());

        let cloned = cfg.clone();
        assert!(cloned.enabled);
        assert_eq!(cloned.endpoint, "http://collector:4317");
        assert_eq!(cloned.headers.get("x-api-key").unwrap(), "secret");
    }

    #[test]
    fn config_debug_format() {
        let cfg = OtlpConfig::default();
        let debug = format!("{cfg:?}");
        assert!(debug.contains("OtlpConfig"));
        assert!(debug.contains("enabled"));
    }
}
