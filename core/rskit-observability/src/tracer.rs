use std::time::Duration;

use opentelemetry::propagation::{Extractor, Injector, TextMapPropagator};
use opentelemetry_sdk::propagation::TraceContextPropagator;
use serde::{Deserialize, Serialize};
use tracing::Span;
use tracing_opentelemetry::OpenTelemetrySpanExt;

use rskit_errors::{AppError, AppResult, ErrorCode};

/// Stable OTel semantic-convention key for service name.
pub const SERVICE_NAME: &str = "service.name";

/// OTLP exporter protocol.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[non_exhaustive]
pub enum OtlpProtocol {
    /// OTLP over gRPC/Tonic, usually port 4317.
    #[default]
    Grpc,
    /// OTLP over HTTP/protobuf, usually port 4318.
    HttpBinary,
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
pub fn tracer_provider(cfg: &TracingConfig) -> AppResult<TracerGuard> {
    tracer_provider_with_protocol(cfg, OtlpProtocol::Grpc)
}

/// Build an injectable OpenTelemetry tracer provider without touching global state.
///
/// This legacy name does not install a process-global subscriber; callers inject
/// [`TracerGuard::tracer`] into their own `tracing_opentelemetry` layer.
pub fn init_tracer(cfg: &TracingConfig) -> AppResult<TracerGuard> {
    tracer_provider(cfg)
}

/// Build an injectable OpenTelemetry tracer provider with an explicit OTLP protocol.
pub fn tracer_provider_with_protocol(
    cfg: &TracingConfig,
    protocol: OtlpProtocol,
) -> AppResult<TracerGuard> {
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

/// Attach bounded operation attributes to an active tracing span.
pub fn set_operation_attributes(
    span: &Span,
    service_name: &str,
    operation_name: &str,
    request_id: &str,
) {
    span.set_attribute(SERVICE_NAME, service_name.to_owned());
    span.set_attribute("operation.name", operation_name.to_owned());
    span.set_attribute("request.id", request_id.to_owned());
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
