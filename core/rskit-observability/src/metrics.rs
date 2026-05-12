use std::time::Duration;

use opentelemetry::metrics::{Counter, Gauge, Histogram, UpDownCounter};
use serde::Deserialize;

use rskit_errors::AppResult;


/// Configuration for the OpenTelemetry metrics pipeline.
#[derive(Debug, Clone, Deserialize)]
pub struct MetricsConfig {
    /// Logical service name attached to every metric.
    pub service_name: String,
    /// How often metrics are exported to the collector.
    pub export_interval: Duration,
    /// OTLP gRPC endpoint. When `None`, metrics are collected but not exported.
    pub otlp_endpoint: Option<String>,
}

/// Handle to the metrics pipeline — use it to create instruments.
pub struct MetricsHandle {
    meter: opentelemetry::metrics::Meter,
    _provider: opentelemetry_sdk::metrics::SdkMeterProvider,
}

impl MetricsHandle {
    /// Create a monotonic `Counter<u64>`.
    pub fn counter(&self, name: impl Into<String>, description: impl Into<String>) -> Counter<u64> {
        self.meter
            .u64_counter(name.into())
            .with_description(description.into())
            .build()
    }

    /// Create a `Histogram<f64>` for latency distributions etc.
    pub fn histogram(
        &self,
        name: impl Into<String>,
        description: impl Into<String>,
    ) -> Histogram<f64> {
        self.meter
            .f64_histogram(name.into())
            .with_description(description.into())
            .build()
    }

    /// Create a `Gauge<f64>` for point-in-time values.
    pub fn gauge(&self, name: impl Into<String>, description: impl Into<String>) -> Gauge<f64> {
        self.meter
            .f64_gauge(name.into())
            .with_description(description.into())
            .build()
    }

    /// Create an `UpDownCounter<i64>` for values that go up and down.
    pub fn up_down_counter(
        &self,
        name: impl Into<String>,
        description: impl Into<String>,
    ) -> UpDownCounter<i64> {
        self.meter
            .i64_up_down_counter(name.into())
            .with_description(description.into())
            .build()
    }
}

/// Initialise an OpenTelemetry metrics pipeline with optional OTLP export.
pub fn init_metrics(cfg: &MetricsConfig) -> AppResult<MetricsHandle> {
    use opentelemetry::metrics::MeterProvider as _;
    use opentelemetry::{InstrumentationScope, KeyValue};
    use opentelemetry_sdk::Resource;
    use opentelemetry_sdk::metrics::SdkMeterProvider;

    let resource = Resource::builder_empty()
        .with_attributes([KeyValue::new("service.name", cfg.service_name.clone())])
        .build();

    #[allow(unused_mut)]
    let mut builder = SdkMeterProvider::builder().with_resource(resource);

    if let Some(_endpoint) = &cfg.otlp_endpoint {
        #[cfg(feature = "otlp")]
        {
            use opentelemetry_otlp::{MetricExporter, WithExportConfig};
            use opentelemetry_sdk::metrics::PeriodicReader;
            use rskit_errors::{AppError, ErrorCode};

            let endpoint = _endpoint;
            let exporter = MetricExporter::builder()
                .with_tonic()
                .with_endpoint(endpoint.clone())
                .build()
                .map_err(|e| {
                    AppError::new(ErrorCode::Internal, format!("OTLP metric exporter: {e}"))
                })?;

            let reader = PeriodicReader::builder(exporter)
                .with_interval(cfg.export_interval)
                .build();

            builder = builder.with_reader(reader);
        }
        #[cfg(not(feature = "otlp"))]
        {
            ::tracing::warn!("OTLP endpoint configured but 'otlp' feature is disabled; metrics will not be exported");
        }
    }

    let provider = builder.build();
    let scope = InstrumentationScope::builder(cfg.service_name.clone()).build();
    let meter = provider.meter_with_scope(scope);

    Ok(MetricsHandle {
        meter,
        _provider: provider,
    })
}
