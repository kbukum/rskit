use std::time::Duration;

use opentelemetry::propagation::{Extractor, Injector, TextMapPropagator};
use opentelemetry_sdk::propagation::TraceContextPropagator;
use serde::{Deserialize, Serialize};
use tracing::Span;
use tracing_opentelemetry::OpenTelemetrySpanExt;

use rskit_errors::{AppError, AppResult, ErrorCode};

use crate::attribute::set_span_attribute;

/// Stable OTel semantic-convention key for service name.
pub const SERVICE_NAME: &str = "service.name";

/// OTLP exporter protocol.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[non_exhaustive]
pub enum OtlpProtocol {
    /// OTLP over HTTP/protobuf, usually port 4318. This is the default transport.
    #[default]
    HttpBinary,
    /// OTLP over gRPC/Tonic, usually port 4317.
    Grpc,
}

/// Configuration for an OTLP trace exporter.
#[derive(Debug, Clone, Deserialize)]
pub struct TracingConfig {
    /// Logical service name reported in every span.
    pub service_name: String,
    /// OTLP collector endpoint for the selected exporter protocol.
    pub endpoint: String,
    /// Sampling probability in `0.0..=1.0`.
    pub sample_rate: f64,
    /// Maximum time to wait when flushing spans on shutdown.
    pub export_timeout: Duration,
}

/// Injectable tracer provider handle.
pub struct TracerGuard {
    provider: opentelemetry_sdk::trace::SdkTracerProvider,
    tracer: opentelemetry_sdk::trace::SdkTracer,
}

impl Drop for TracerGuard {
    fn drop(&mut self) {
        if let Err(e) = self.provider.shutdown() {
            tracing::warn!(error = %e, "failed to shutdown tracer provider");
        }
    }
}

impl TracerGuard {
    /// Return the SDK tracer provider for constructor injection.
    #[must_use]
    pub fn provider(&self) -> opentelemetry_sdk::trace::SdkTracerProvider {
        self.provider.clone()
    }

    /// Return a tracer that can be injected into `tracing_opentelemetry::layer()`.
    #[must_use]
    pub fn tracer(&self) -> opentelemetry_sdk::trace::SdkTracer {
        self.tracer.clone()
    }
}

/// Build an injectable OpenTelemetry tracer provider without touching global state.
///
/// Uses the default OTLP protocol ([`OtlpProtocol::HttpBinary`]).
pub fn tracer_provider(cfg: &TracingConfig) -> AppResult<TracerGuard> {
    tracer_provider_with_protocol(cfg, OtlpProtocol::default())
}

/// Build an injectable OpenTelemetry tracer provider with an explicit OTLP protocol.
pub fn tracer_provider_with_protocol(
    cfg: &TracingConfig,
    protocol: OtlpProtocol,
) -> AppResult<TracerGuard> {
    #[cfg(not(feature = "otlp"))]
    {
        let _ = (cfg, protocol);
        Err(AppError::new(
            ErrorCode::InvalidInput,
            "OTLP tracing exporter requires the `otlp` feature",
        ))
    }

    #[cfg(feature = "otlp")]
    {
        use opentelemetry::trace::TracerProvider as _;
        use opentelemetry::{InstrumentationScope, KeyValue};
        use opentelemetry_otlp::{SpanExporter, WithExportConfig};
        use opentelemetry_sdk::Resource;
        use opentelemetry_sdk::trace::{Sampler, SdkTracerProvider};
        let exporter = match protocol {
            OtlpProtocol::Grpc => SpanExporter::builder()
                .with_tonic()
                .with_endpoint(&cfg.endpoint)
                .with_timeout(cfg.export_timeout)
                .build(),
            OtlpProtocol::HttpBinary => SpanExporter::builder()
                .with_http()
                .with_protocol(opentelemetry_otlp::Protocol::HttpBinary)
                .with_endpoint(&cfg.endpoint)
                .with_timeout(cfg.export_timeout)
                .build(),
        }
        .map_err(|e| AppError::new(ErrorCode::Internal, format!("OTLP span exporter: {e}")))?;

        let sampler = if (cfg.sample_rate - 1.0).abs() < f64::EPSILON {
            Sampler::AlwaysOn
        } else if cfg.sample_rate <= 0.0 {
            Sampler::AlwaysOff
        } else {
            Sampler::TraceIdRatioBased(cfg.sample_rate)
        };

        let resource = Resource::builder_empty()
            .with_attributes([KeyValue::new(SERVICE_NAME, cfg.service_name.clone())])
            .build();

        let provider = SdkTracerProvider::builder()
            .with_batch_exporter(exporter)
            .with_sampler(sampler)
            .with_resource(resource)
            .build();

        let scope = InstrumentationScope::builder(cfg.service_name.clone()).build();
        let tracer = provider.tracer_with_scope(scope);
        Ok(TracerGuard { provider, tracer })
    }
}

/// Attach bounded operation attributes to an active tracing span.
pub fn set_operation_attributes(
    span: &Span,
    service_name: &str,
    operation_name: &str,
    request_id: &str,
) {
    set_span_attribute(span, SERVICE_NAME, service_name);
    set_span_attribute(span, "operation.name", operation_name);
    set_span_attribute(span, "request.id", request_id);
}

/// Inject the current trace context into HTTP headers (W3C Trace-Context).
pub fn inject_trace_context(headers: &mut http::HeaderMap) {
    let propagator = TraceContextPropagator::new();
    let cx = Span::current().context();
    propagator.inject_context(&cx, &mut HeaderMapCarrier(headers));
}

/// Extract a trace context from incoming HTTP headers.
pub fn extract_trace_context(headers: &http::HeaderMap) -> opentelemetry::Context {
    let propagator = TraceContextPropagator::new();
    propagator.extract(&HeaderMapExtractor(headers))
}

// ---- carriers ---------------------------------------------------------------

struct HeaderMapCarrier<'a>(&'a mut http::HeaderMap);

impl Injector for HeaderMapCarrier<'_> {
    fn set(&mut self, key: &str, value: String) {
        if let Ok(name) = http::header::HeaderName::from_bytes(key.as_bytes())
            && let Ok(val) = http::header::HeaderValue::from_str(&value)
        {
            self.0.insert(name, val);
        }
    }
}

struct HeaderMapExtractor<'a>(&'a http::HeaderMap);

impl Extractor for HeaderMapExtractor<'_> {
    fn get(&self, key: &str) -> Option<&str> {
        self.0.get(key).and_then(|v| v.to_str().ok())
    }

    fn keys(&self) -> Vec<&str> {
        self.0.keys().map(|k| k.as_str()).collect()
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    fn config() -> TracingConfig {
        TracingConfig {
            service_name: "test-service".to_string(),
            endpoint: "http://127.0.0.1:4317".to_string(),
            sample_rate: 1.0,
            export_timeout: Duration::from_millis(10),
        }
    }

    #[test]
    fn otlp_protocol_round_trips_through_serde() {
        let grpc = serde_json::to_string(&OtlpProtocol::Grpc).expect("serialize grpc");
        let http = serde_json::to_string(&OtlpProtocol::HttpBinary).expect("serialize http");

        assert_eq!(
            serde_json::from_str::<OtlpProtocol>(&grpc).expect("deserialize grpc"),
            OtlpProtocol::Grpc
        );
        assert_eq!(
            serde_json::from_str::<OtlpProtocol>(&http).expect("deserialize http"),
            OtlpProtocol::HttpBinary
        );
    }

    #[cfg(not(feature = "otlp"))]
    #[test]
    fn tracer_provider_requires_otlp_feature_for_exporter() {
        let error = match tracer_provider_with_protocol(&config(), OtlpProtocol::Grpc) {
            Ok(_) => panic!("otlp disabled should reject exporter configuration"),
            Err(error) => error,
        };

        assert_eq!(error.code(), ErrorCode::InvalidInput);
        assert!(error.message().contains("otlp"));
    }

    #[cfg(feature = "otlp")]
    #[tokio::test]
    async fn tracer_provider_builds_exporters_for_supported_protocols() {
        for (protocol, sample_rate) in [
            (OtlpProtocol::Grpc, 1.0),
            (OtlpProtocol::HttpBinary, 0.0),
            (OtlpProtocol::HttpBinary, 0.5),
        ] {
            let mut cfg = config();
            cfg.sample_rate = sample_rate;
            let guard =
                tracer_provider_with_protocol(&cfg, protocol).expect("build tracer provider");
            let _provider = guard.provider();
            let _tracer = guard.tracer();
        }
    }

    #[test]
    fn operation_attributes_and_trace_context_helpers_are_tolerant() {
        let span = tracing::info_span!("operation");
        set_operation_attributes(&span, "svc", "op", "req-1");

        let mut headers = http::HeaderMap::new();
        headers.insert(
            "traceparent",
            http::HeaderValue::from_static(
                "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01",
            ),
        );
        headers.insert("tracestate", http::HeaderValue::from_static("vendor=value"));
        let _context = extract_trace_context(&headers);

        headers.insert(
            "traceparent",
            http::HeaderValue::from_bytes(b"\xff").expect("invalid utf8 header value is allowed"),
        );
        let _context = extract_trace_context(&headers);

        let _entered = span.enter();
        let mut injected = http::HeaderMap::new();
        inject_trace_context(&mut injected);
    }

    #[test]
    fn header_carriers_ignore_invalid_injected_values_and_list_valid_keys() {
        let mut headers = http::HeaderMap::new();
        let mut injector = HeaderMapCarrier(&mut headers);
        injector.set("UPPERCASE", "ok".to_string());
        injector.set("valid-header", "\ninvalid".to_string());
        assert!(headers.contains_key("uppercase"));
        assert!(!headers.contains_key("valid-header"));

        let extractor = HeaderMapExtractor(&headers);
        assert_eq!(extractor.get("uppercase"), Some("ok"));
        assert!(extractor.keys().contains(&"uppercase"));
    }
}
