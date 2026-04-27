use std::time::Duration;

use opentelemetry::propagation::{Injector, TextMapPropagator};
use opentelemetry_sdk::propagation::TraceContextPropagator;
use serde::Deserialize;
use tracing::Span;
use tracing_opentelemetry::OpenTelemetrySpanExt;

use rskit_errors::{AppError, AppResult, ErrorCode};

/// Guards against registering the global tracing subscriber more than once.
/// `try_init()` returns an error on the second call; this lock makes `init_tracer`
/// fully idempotent — subsequent calls are silently skipped.
static SUBSCRIBER_INIT: std::sync::OnceLock<()> = std::sync::OnceLock::new();

/// Configuration for an OTLP trace exporter.
#[derive(Debug, Clone, Deserialize)]
pub struct TracingConfig {
    /// Logical service name reported in every span.
    pub service_name: String,
    /// OTLP gRPC collector endpoint (e.g. `http://localhost:4317`).
    pub endpoint: String,
    /// Sampling probability in `0.0..=1.0`.
    pub sample_rate: f64,
    /// Maximum time to wait when flushing spans on shutdown.
    pub export_timeout: Duration,
}

/// RAII guard — shuts down the tracer provider on drop.
pub struct TracerGuard {
    _provider: opentelemetry_sdk::trace::SdkTracerProvider,
}

impl Drop for TracerGuard {
    fn drop(&mut self) {
        if let Err(e) = self._provider.shutdown() {
            tracing::warn!(error = %e, "failed to shutdown tracer provider");
        }
    }
}

/// Initialise an OpenTelemetry tracer with an OTLP exporter and install
/// a `tracing` layer that bridges spans into OpenTelemetry.
///
/// # `RUST_LOG` interaction
///
/// If a `RUST_LOG` environment variable is set at runtime it will be respected
/// by the installed `tracing-subscriber` filter.  Note that this can override
/// the sampling and verbosity configured via [`TracingConfig`].  Operators
/// should be aware that setting `RUST_LOG` in production may increase span
/// volume and override the intended sample rate.
pub fn init_tracer(cfg: &TracingConfig) -> AppResult<TracerGuard> {
    use opentelemetry::trace::TracerProvider as _;
    use opentelemetry::{InstrumentationScope, KeyValue};
    use opentelemetry_otlp::{SpanExporter, WithExportConfig};
    use opentelemetry_sdk::Resource;
    use opentelemetry_sdk::trace::{Sampler, SdkTracerProvider};
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;

    let exporter = SpanExporter::builder()
        .with_tonic()
        .with_endpoint(&cfg.endpoint)
        .with_timeout(cfg.export_timeout)
        .build()
        .map_err(|e| AppError::new(ErrorCode::Internal, format!("OTLP span exporter: {e}")))?;

    let sampler = if (cfg.sample_rate - 1.0).abs() < f64::EPSILON {
        Sampler::AlwaysOn
    } else if cfg.sample_rate <= 0.0 {
        Sampler::AlwaysOff
    } else {
        Sampler::TraceIdRatioBased(cfg.sample_rate)
    };

    let resource = Resource::builder_empty()
        .with_attributes([KeyValue::new("service.name", cfg.service_name.clone())])
        .build();

    let provider = SdkTracerProvider::builder()
        .with_batch_exporter(exporter)
        .with_sampler(sampler)
        .with_resource(resource)
        .build();

    let scope = InstrumentationScope::builder(cfg.service_name.clone()).build();
    let tracer = provider.tracer_with_scope(scope);
    let otel_layer = tracing_opentelemetry::layer().with_tracer(tracer);

    // IDEMPOTENCY(#28): only install the global subscriber once; subsequent
    // calls (e.g. in tests or hot-reload scenarios) are silently skipped.
    SUBSCRIBER_INIT.get_or_init(|| {
        if let Err(e) = tracing_subscriber::registry().with(otel_layer).try_init() {
            tracing::warn!(error = %e, "tracing subscriber already initialised — skipping");
        }
    });

    Ok(TracerGuard {
        _provider: provider,
    })
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

impl opentelemetry::propagation::Extractor for HeaderMapExtractor<'_> {
    fn get(&self, key: &str) -> Option<&str> {
        self.0.get(key).and_then(|v| v.to_str().ok())
    }

    fn keys(&self) -> Vec<&str> {
        self.0.keys().map(|k| k.as_str()).collect()
    }
}
