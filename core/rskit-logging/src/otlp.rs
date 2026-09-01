//! OpenTelemetry Logs bridge with OTLP export.
//!
//! Bridges [`tracing`] events to the OpenTelemetry Logs SDK
//! so they can be exported via OTLP (gRPC or HTTP) to a collector.
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

use opentelemetry::KeyValue;
use opentelemetry_appender_tracing::layer::OpenTelemetryTracingBridge;
use opentelemetry_otlp::{WithExportConfig, WithHttpConfig};
use opentelemetry_sdk::Resource;
use opentelemetry_sdk::logs::{SdkLogger, SdkLoggerProvider};
use tracing::Subscriber;
use tracing_subscriber::registry::LookupSpan;

pub use crate::config::OtlpConfig;
use crate::error::{self, LoggingResult};

// ── Config ──────────────────────────────────────────────────────────────────

// ── Provider ────────────────────────────────────────────────────────────────

/// Manages the OpenTelemetry [`SdkLoggerProvider`] for OTLP export.
///
/// Create via [`OtlpProvider::new`],
/// then call [`OtlpProvider::layer`] to obtain a [`tracing_subscriber::Layer`] that can be composed into the subscriber stack.
///
/// The provider **must** be shut down gracefully via [`OtlpProvider::shutdown`] (or by dropping the [`crate::LoggingGuard`]) to flush pending log records.
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
    ) -> LoggingResult<Option<Self>> {
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

    /// Get the OpenTelemetry tracing layer for use with [`tracing_subscriber`].
    ///
    /// The returned layer converts every [`tracing`] event into an OpenTelemetry log record
    /// and forwards it to the OTLP exporter.
    pub fn layer<S>(&self) -> OpenTelemetryTracingBridge<SdkLoggerProvider, SdkLogger>
    where
        S: Subscriber + for<'a> LookupSpan<'a>,
    {
        OpenTelemetryTracingBridge::new(&self.provider)
    }

    /// Gracefully shut down the provider, flushing all pending log records.
    pub fn shutdown(self) -> LoggingResult<()> {
        self.provider.shutdown().map_err(error::otlp_shutdown)
    }
}

// ── Internal helpers ────────────────────────────────────────────────────────

/// Build an OTLP [`LogExporter`](opentelemetry_otlp::LogExporter) from config.
fn build_exporter(cfg: &OtlpConfig) -> LoggingResult<opentelemetry_otlp::LogExporter> {
    let endpoint = resolve_endpoint(cfg)?;
    match cfg.protocol.as_str() {
        "http" => {
            let exporter = opentelemetry_otlp::LogExporter::builder()
                .with_http()
                .with_endpoint(&endpoint)
                .with_headers(cfg.headers.clone())
                .build()
                .map_err(error::otlp_exporter)?;
            Ok(exporter)
        }
        "grpc" => {
            if !cfg.headers.is_empty() {
                return Err(error::grpc_headers_not_supported());
            }
            let exporter = opentelemetry_otlp::LogExporter::builder()
                .with_tonic()
                .with_endpoint(&endpoint)
                .build()
                .map_err(error::otlp_exporter)?;
            Ok(exporter)
        }
        other => Err(error::invalid_protocol(other)),
    }
}

/// Resolve the exporter endpoint, applying the `insecure` transport policy.
///
/// Transport security is carried by the endpoint URL scheme (there is no separate TLS toggle in
/// the OTLP builder), so `insecure` is authoritative for the scheme. A scheme-less endpoint is
/// prefixed with `http://` when `insecure` is set and `https://` otherwise. An explicit scheme
/// that contradicts the flag is rejected rather than silently honored: `https://` with
/// `insecure = true`, and `http://` with `insecure = false` (which would send records and headers
/// in plaintext despite the secure default).
fn resolve_endpoint(cfg: &OtlpConfig) -> LoggingResult<String> {
    let endpoint = cfg.endpoint.trim();
    if has_scheme(endpoint, "https://") {
        if cfg.insecure {
            return Err(error::insecure_conflicts_with_endpoint(endpoint));
        }
        Ok(endpoint.to_string())
    } else if has_scheme(endpoint, "http://") {
        if !cfg.insecure {
            return Err(error::secure_requires_https_endpoint(endpoint));
        }
        Ok(endpoint.to_string())
    } else {
        let scheme = if cfg.insecure { "http://" } else { "https://" };
        Ok(format!("{scheme}{endpoint}"))
    }
}

/// Returns `true` when `endpoint` begins with `scheme` (e.g. `"https://"`).
///
/// URI schemes are case-insensitive (RFC 3986 §3.1), so the comparison is case-insensitive while
/// the original endpoint is left untouched; a mixed-case `HTTPS://collector:4317` is recognized as
/// having an explicit scheme rather than being treated as scheme-less and double-prefixed.
fn has_scheme(endpoint: &str, scheme: &str) -> bool {
    endpoint
        .as_bytes()
        .get(..scheme.len())
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case(scheme.as_bytes()))
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_is_disabled() {
        let cfg = OtlpConfig::default();
        assert!(!cfg.enabled);
        assert_eq!(cfg.endpoint, "localhost:4317");
        assert_eq!(cfg.protocol, "grpc");
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
        let mut cfg = OtlpConfig {
            enabled: true,
            endpoint: "http://collector:4317".to_string(),
            ..Default::default()
        };
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

    #[test]
    fn invalid_protocol_returns_typed_error() {
        let cfg = OtlpConfig {
            enabled: true,
            protocol: "udp".to_string(),
            ..Default::default()
        };
        let err = match OtlpProvider::new(&cfg, "test-svc", "test", "0.1.0") {
            Ok(_) => panic!("unsupported protocol must fail"),
            Err(err) => err,
        };
        assert_eq!(err.code(), rskit_errors::ErrorCode::InvalidInput);
    }

    #[test]
    fn grpc_headers_return_typed_error() {
        let mut cfg = OtlpConfig {
            enabled: true,
            ..Default::default()
        };
        cfg.headers
            .insert("x-api-key".to_string(), "secret".to_string());
        let err = match OtlpProvider::new(&cfg, "test-svc", "test", "0.1.0") {
            Ok(_) => panic!("grpc headers are unsupported"),
            Err(err) => err,
        };
        assert_eq!(err.code(), rskit_errors::ErrorCode::InvalidInput);
    }

    #[test]
    fn insecure_prefixes_scheme_less_endpoint_with_http() {
        let cfg = OtlpConfig {
            endpoint: "collector:4317".to_string(),
            insecure: true,
            ..Default::default()
        };
        assert_eq!(resolve_endpoint(&cfg).unwrap(), "http://collector:4317");
    }

    #[test]
    fn secure_prefixes_scheme_less_endpoint_with_https() {
        let cfg = OtlpConfig {
            endpoint: "collector:4317".to_string(),
            insecure: false,
            ..Default::default()
        };
        assert_eq!(resolve_endpoint(&cfg).unwrap(), "https://collector:4317");
    }

    #[test]
    fn insecure_conflicting_with_https_endpoint_is_rejected() {
        let cfg = OtlpConfig {
            endpoint: "https://collector:4317".to_string(),
            insecure: true,
            ..Default::default()
        };
        let err = resolve_endpoint(&cfg).unwrap_err();
        assert_eq!(err.code(), rskit_errors::ErrorCode::InvalidInput);
    }

    #[test]
    fn secure_endpoint_with_explicit_http_is_rejected() {
        let cfg = OtlpConfig {
            endpoint: "http://collector:4317".to_string(),
            insecure: false,
            ..Default::default()
        };
        let err = resolve_endpoint(&cfg).unwrap_err();
        assert_eq!(err.code(), rskit_errors::ErrorCode::InvalidInput);
    }

    #[test]
    fn explicit_scheme_endpoints_are_preserved() {
        let insecure = OtlpConfig {
            endpoint: "http://collector:4317".to_string(),
            insecure: true,
            ..Default::default()
        };
        assert_eq!(
            resolve_endpoint(&insecure).unwrap(),
            "http://collector:4317"
        );

        let secure = OtlpConfig {
            endpoint: "https://collector:4317".to_string(),
            insecure: false,
            ..Default::default()
        };
        assert_eq!(resolve_endpoint(&secure).unwrap(), "https://collector:4317");
    }

    #[test]
    fn uppercase_scheme_is_recognized_case_insensitively_and_preserved() {
        // URI schemes are case-insensitive, so an explicit mixed-case scheme must be honored
        // (not treated as scheme-less and double-prefixed) while the original endpoint is kept.
        let secure = OtlpConfig {
            endpoint: "HTTPS://collector:4317".to_string(),
            insecure: false,
            ..Default::default()
        };
        assert_eq!(resolve_endpoint(&secure).unwrap(), "HTTPS://collector:4317");

        let insecure = OtlpConfig {
            endpoint: "Http://collector:4317".to_string(),
            insecure: true,
            ..Default::default()
        };
        assert_eq!(
            resolve_endpoint(&insecure).unwrap(),
            "Http://collector:4317"
        );
    }

    #[test]
    fn uppercase_scheme_conflicting_with_flag_is_rejected() {
        let insecure_https = OtlpConfig {
            endpoint: "HTTPS://collector:4317".to_string(),
            insecure: true,
            ..Default::default()
        };
        assert_eq!(
            resolve_endpoint(&insecure_https).unwrap_err().code(),
            rskit_errors::ErrorCode::InvalidInput
        );

        let secure_http = OtlpConfig {
            endpoint: "HTTP://collector:4317".to_string(),
            insecure: false,
            ..Default::default()
        };
        assert_eq!(
            resolve_endpoint(&secure_http).unwrap_err().code(),
            rskit_errors::ErrorCode::InvalidInput
        );
    }

    #[tokio::test]
    async fn grpc_exporter_builds_with_secure_https_endpoint() {
        // The secure-by-default grpc transport resolves to an `https://` endpoint. Building the
        // tonic exporter against it must succeed, which only holds when a TLS feature is compiled
        // into `opentelemetry-otlp` — otherwise the tonic channel has no TLS backend for the https
        // URL. Runs inside a Tokio runtime because the tonic channel needs a reactor.
        let cfg = OtlpConfig {
            enabled: true,
            endpoint: "collector:4317".to_string(),
            protocol: "grpc".to_string(),
            insecure: false,
            ..Default::default()
        };
        assert_eq!(resolve_endpoint(&cfg).unwrap(), "https://collector:4317");
        build_exporter(&cfg).expect("grpc exporter must build against an https endpoint");
    }
}
