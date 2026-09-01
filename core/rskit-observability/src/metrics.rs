use std::time::Duration;

use opentelemetry::metrics::{Counter, Gauge, Histogram, UpDownCounter};
use serde::Deserialize;

use rskit_errors::{AppError, AppResult, ErrorCode};

use crate::tracer::{OtlpProtocol, SERVICE_NAME};

/// Configuration for the OpenTelemetry metrics pipeline.
#[derive(Debug, Clone, Deserialize)]
pub struct MetricsConfig {
    /// Logical service name attached to every metric.
    pub service_name: String,
    /// How often metrics are exported to the collector.
    pub export_interval: Duration,
    /// OTLP endpoint. When `None`, metrics are collected but not exported.
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

    /// Create a `Histogram<f64>` that declares an OpenTelemetry unit (e.g. `s` for seconds).
    ///
    /// Exporters use the unit to interpret and convert the recorded distribution.
    pub fn histogram_with_unit(
        &self,
        name: impl Into<String>,
        description: impl Into<String>,
        unit: impl Into<String>,
    ) -> Histogram<f64> {
        self.meter
            .f64_histogram(name.into())
            .with_description(description.into())
            .with_unit(unit.into())
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
///
/// Uses the default OTLP protocol ([`OtlpProtocol::HttpBinary`]).
pub fn init_metrics(cfg: &MetricsConfig) -> AppResult<MetricsHandle> {
    init_metrics_with_protocol(cfg, OtlpProtocol::default())
}

/// Initialise an OpenTelemetry metrics pipeline with an explicit OTLP protocol.
pub fn init_metrics_with_protocol(
    cfg: &MetricsConfig,
    protocol: OtlpProtocol,
) -> AppResult<MetricsHandle> {
    use opentelemetry::metrics::MeterProvider as _;
    use opentelemetry::{InstrumentationScope, KeyValue};
    use opentelemetry_sdk::Resource;
    use opentelemetry_sdk::metrics::SdkMeterProvider;

    let resource = Resource::builder_empty()
        .with_attributes([KeyValue::new(SERVICE_NAME, cfg.service_name.clone())])
        .build();

    let builder = SdkMeterProvider::builder().with_resource(resource);

    if let Some(endpoint) = &cfg.otlp_endpoint {
        #[cfg(not(feature = "otlp"))]
        {
            let _ = (endpoint, protocol);
            Err(AppError::new(
                ErrorCode::InvalidInput,
                "OTLP metric exporter requires the `otlp` feature",
            ))?;
        }

        #[cfg(feature = "otlp")]
        {
            use opentelemetry_otlp::{MetricExporter, WithExportConfig};
            use opentelemetry_sdk::metrics::PeriodicReader;

            let exporter = match protocol {
                OtlpProtocol::Grpc => MetricExporter::builder()
                    .with_tonic()
                    .with_endpoint(endpoint)
                    .build(),
                OtlpProtocol::HttpBinary => MetricExporter::builder()
                    .with_http()
                    .with_protocol(opentelemetry_otlp::Protocol::HttpBinary)
                    .with_endpoint(endpoint)
                    .build(),
            }
            .map_err(|e| {
                AppError::new(ErrorCode::Internal, format!("OTLP metric exporter: {e}"))
            })?;

            let reader = PeriodicReader::builder(exporter)
                .with_interval(cfg.export_interval)
                .build();

            let builder = builder.with_reader(reader);
            let provider = builder.build();
            let scope = InstrumentationScope::builder(cfg.service_name.clone()).build();
            let meter = provider.meter_with_scope(scope);

            return Ok(MetricsHandle {
                meter,
                _provider: provider,
            });
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

/// Canonical metric names shared across the toolkit and sibling kits.
pub mod metric_name {
    /// Total number of requests.
    pub const REQUEST_TOTAL: &str = "request.total";
    /// Request duration in seconds.
    pub const REQUEST_DURATION: &str = "request.duration";
    /// Number of currently active requests.
    pub const REQUEST_ACTIVE: &str = "request.active";
    /// Total number of operations.
    pub const OPERATION_TOTAL: &str = "operation.total";
    /// Operation duration in seconds.
    pub const OPERATION_DURATION: &str = "operation.duration";
    /// Total errors by type and component.
    pub const ERROR_TOTAL: &str = "error.total";
}

/// Canonical metric attribute keys shared across the toolkit and sibling kits.
pub mod metric_attr {
    /// Logical service name.
    pub const SERVICE: &str = "service";
    /// Request method or route.
    pub const METHOD: &str = "method";
    /// Terminal status label.
    pub const STATUS: &str = "status";
    /// Operation name.
    pub const OPERATION: &str = "operation";
    /// Error type label.
    pub const TYPE: &str = "type";
    /// Owning component label.
    pub const COMPONENT: &str = "component";
}

/// A completed request recorded through [`Metrics::record_request_end`].
#[derive(Debug, Clone)]
pub struct RequestMetric {
    /// Logical service name.
    pub service: String,
    /// Request method or route.
    pub method: String,
    /// Terminal status label.
    pub status: String,
    /// Wall-clock duration of the request.
    pub duration: Duration,
}

/// An executed operation recorded through [`Metrics::record_operation`].
#[derive(Debug, Clone)]
pub struct OperationMetric {
    /// Logical service name.
    pub service: String,
    /// Operation name.
    pub operation: String,
    /// Terminal status label.
    pub status: String,
    /// Wall-clock duration of the operation.
    pub duration: Duration,
}

/// Canonical service observability instruments with shared metric names and attributes.
///
/// Construct one from a [`MetricsHandle`] to record requests, operations, and errors under the
/// cross-kit metric names in [`metric_name`] with the attribute keys in [`metric_attr`].
pub struct Metrics {
    request_total: Counter<u64>,
    request_duration: Histogram<f64>,
    request_active: UpDownCounter<i64>,
    operation_total: Counter<u64>,
    operation_duration: Histogram<f64>,
    error_total: Counter<u64>,
}

impl Metrics {
    /// Create the canonical instrument set on the given metrics handle.
    #[must_use]
    pub fn new(handle: &MetricsHandle) -> Self {
        Self {
            request_total: handle.counter(metric_name::REQUEST_TOTAL, "Total number of requests"),
            request_duration: handle.histogram_with_unit(
                metric_name::REQUEST_DURATION,
                "Duration of requests in seconds",
                "s",
            ),
            request_active: handle.up_down_counter(
                metric_name::REQUEST_ACTIVE,
                "Number of currently active requests",
            ),
            operation_total: handle
                .counter(metric_name::OPERATION_TOTAL, "Total number of operations"),
            operation_duration: handle.histogram_with_unit(
                metric_name::OPERATION_DURATION,
                "Duration of operations in seconds",
                "s",
            ),
            error_total: handle.counter(
                metric_name::ERROR_TOTAL,
                "Total errors by type and component",
            ),
        }
    }

    /// Increment the active request count.
    pub fn record_request_start(&self) {
        self.request_active.add(1, &[]);
    }

    /// Decrement active requests and record the completed request.
    pub fn record_request_end(&self, request: &RequestMetric) {
        use opentelemetry::KeyValue;
        self.request_active.add(-1, &[]);
        self.request_total.add(
            1,
            &[
                KeyValue::new(metric_attr::SERVICE, request.service.clone()),
                KeyValue::new(metric_attr::METHOD, request.method.clone()),
                KeyValue::new(metric_attr::STATUS, request.status.clone()),
            ],
        );
        self.request_duration.record(
            request.duration.as_secs_f64(),
            &[
                KeyValue::new(metric_attr::SERVICE, request.service.clone()),
                KeyValue::new(metric_attr::METHOD, request.method.clone()),
            ],
        );
    }

    /// Record an executed operation.
    pub fn record_operation(&self, operation: &OperationMetric) {
        use opentelemetry::KeyValue;
        self.operation_total.add(
            1,
            &[
                KeyValue::new(metric_attr::SERVICE, operation.service.clone()),
                KeyValue::new(metric_attr::OPERATION, operation.operation.clone()),
                KeyValue::new(metric_attr::STATUS, operation.status.clone()),
            ],
        );
        self.operation_duration.record(
            operation.duration.as_secs_f64(),
            &[
                KeyValue::new(metric_attr::SERVICE, operation.service.clone()),
                KeyValue::new(metric_attr::OPERATION, operation.operation.clone()),
            ],
        );
    }

    /// Record an error by type and owning component.
    pub fn record_error(&self, error_type: impl Into<String>, component: impl Into<String>) {
        use opentelemetry::KeyValue;
        self.error_total.add(
            1,
            &[
                KeyValue::new(metric_attr::TYPE, error_type.into()),
                KeyValue::new(metric_attr::COMPONENT, component.into()),
            ],
        );
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    fn config(endpoint: Option<&str>) -> MetricsConfig {
        MetricsConfig {
            service_name: "metrics-test".to_string(),
            export_interval: Duration::from_millis(50),
            otlp_endpoint: endpoint.map(str::to_string),
        }
    }

    #[test]
    fn no_export_metrics_pipeline_creates_all_instrument_kinds() {
        let metrics = init_metrics(&config(None)).expect("build metrics provider");

        metrics.counter("requests", "request count").add(1, &[]);
        metrics
            .histogram("latency", "request latency")
            .record(12.5, &[]);
        metrics.gauge("load", "current load").record(0.75, &[]);
        metrics
            .up_down_counter("inflight", "inflight requests")
            .add(1, &[]);
    }

    #[test]
    fn canonical_metric_names_and_attributes_are_stable() {
        assert_eq!(metric_name::REQUEST_TOTAL, "request.total");
        assert_eq!(metric_name::REQUEST_DURATION, "request.duration");
        assert_eq!(metric_name::REQUEST_ACTIVE, "request.active");
        assert_eq!(metric_name::OPERATION_TOTAL, "operation.total");
        assert_eq!(metric_name::OPERATION_DURATION, "operation.duration");
        assert_eq!(metric_name::ERROR_TOTAL, "error.total");
        assert_eq!(metric_attr::SERVICE, "service");
        assert_eq!(metric_attr::METHOD, "method");
        assert_eq!(metric_attr::STATUS, "status");
        assert_eq!(metric_attr::OPERATION, "operation");
        assert_eq!(metric_attr::TYPE, "type");
        assert_eq!(metric_attr::COMPONENT, "component");
    }

    #[test]
    fn canonical_metrics_record_without_exporter() {
        let handle = init_metrics(&config(None)).expect("build metrics provider");
        let metrics = Metrics::new(&handle);

        metrics.record_request_start();
        metrics.record_request_end(&RequestMetric {
            service: "svc".to_string(),
            method: "GET /x".to_string(),
            status: "ok".to_string(),
            duration: Duration::from_millis(12),
        });
        metrics.record_operation(&OperationMetric {
            service: "svc".to_string(),
            operation: "encode".to_string(),
            status: "ok".to_string(),
            duration: Duration::from_millis(5),
        });
        metrics.record_error("timeout", "storage");
    }

    #[cfg(not(feature = "otlp"))]
    #[test]
    fn otlp_endpoint_requires_feature() {
        let error = match init_metrics_with_protocol(
            &config(Some("http://127.0.0.1:4317")),
            OtlpProtocol::Grpc,
        ) {
            Ok(_) => panic!("otlp disabled should reject metric exporter configuration"),
            Err(error) => error,
        };

        assert_eq!(error.code(), ErrorCode::InvalidInput);
        assert!(error.message().contains("otlp"));
    }

    #[cfg(feature = "otlp")]
    #[tokio::test]
    async fn otlp_metrics_pipeline_builds_for_supported_protocols() {
        for protocol in [OtlpProtocol::Grpc, OtlpProtocol::HttpBinary] {
            let metrics =
                init_metrics_with_protocol(&config(Some("http://127.0.0.1:4317")), protocol)
                    .expect("build otlp metrics provider");
            metrics.counter("requests", "request count").add(1, &[]);
        }
    }
}
