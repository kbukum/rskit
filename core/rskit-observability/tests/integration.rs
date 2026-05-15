use std::time::Duration;

use rskit_observability::{LogsConfig, MetricsConfig, OtlpProtocol, TracingConfig};

// ── TracingConfig ───────────────────────────────────────────────────────────

#[test]
fn tracing_config_construction() {
    let cfg = TracingConfig {
        service_name: "my-service".into(),
        endpoint: "http://localhost:4317".into(),
        sample_rate: 0.5,
        export_timeout: Duration::from_secs(10),
    };
    assert_eq!(cfg.service_name, "my-service");
    assert_eq!(cfg.endpoint, "http://localhost:4317");
    assert!((cfg.sample_rate - 0.5).abs() < f64::EPSILON);
    assert_eq!(cfg.export_timeout, Duration::from_secs(10));
}

#[test]
fn tracing_config_full_sample_rate() {
    let cfg = TracingConfig {
        service_name: "svc".into(),
        endpoint: "http://collector:4317".into(),
        sample_rate: 1.0,
        export_timeout: Duration::from_secs(5),
    };
    assert!((cfg.sample_rate - 1.0).abs() < f64::EPSILON);
}

// ── MetricsConfig ───────────────────────────────────────────────────────────

#[test]
fn metrics_config_construction() {
    let cfg = MetricsConfig {
        service_name: "my-service".into(),
        export_interval: Duration::from_secs(15),
        otlp_endpoint: Some("http://localhost:4317".into()),
    };
    assert_eq!(cfg.service_name, "my-service");
    assert_eq!(cfg.export_interval, Duration::from_secs(15));
    assert_eq!(cfg.otlp_endpoint.as_deref(), Some("http://localhost:4317"));
}

#[test]
fn metrics_config_without_endpoint() {
    let cfg = MetricsConfig {
        service_name: "local".into(),
        export_interval: Duration::from_secs(60),
        otlp_endpoint: None,
    };
    assert!(cfg.otlp_endpoint.is_none());
}

#[test]
fn logs_config_supports_http_otlp_protocol() {
    let cfg = LogsConfig {
        service_name: "local".into(),
        otlp_endpoint: None,
        protocol: OtlpProtocol::HttpBinary,
        export_timeout: Duration::from_secs(5),
    };
    let handle = rskit_observability::init_logs(&cfg).unwrap();
    assert!(handle.shutdown().is_ok());
}

// ── Propagation (no external dependency) ────────────────────────────────────

#[test]
fn inject_trace_context_into_empty_headers() {
    let mut headers = http::HeaderMap::new();
    rskit_observability::inject_trace_context(&mut headers);
    // With no active span the propagator may or may not inject traceparent;
    // we just verify it doesn't panic.
}

#[test]
fn extract_trace_context_from_empty_headers() {
    let headers = http::HeaderMap::new();
    let ctx = rskit_observability::extract_trace_context(&headers);
    // Returns a valid (root) context even without traceparent header.
    let _ = ctx;
}

#[test]
fn extract_trace_context_with_traceparent() {
    let mut headers = http::HeaderMap::new();
    headers.insert(
        "traceparent",
        "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01"
            .parse()
            .unwrap(),
    );
    let ctx = rskit_observability::extract_trace_context(&headers);
    let _ = ctx;
}

// ── OTLP-dependent tests ────────────────────────────────────────────────────

#[tokio::test]
#[ignore = "installs global subscriber; requires OTLP collector"]
async fn init_tracer_with_valid_endpoint() {
    let cfg = TracingConfig {
        service_name: "test-svc".into(),
        endpoint: "http://localhost:4317".into(),
        sample_rate: 1.0,
        export_timeout: Duration::from_secs(5),
    };
    let _guard = rskit_observability::init_tracer(&cfg).unwrap();
}

#[tokio::test]
#[ignore = "installs global subscriber; requires OTLP collector"]
async fn init_tracer_is_idempotent() {
    let cfg = TracingConfig {
        service_name: "test-svc".into(),
        endpoint: "http://localhost:4317".into(),
        sample_rate: 1.0,
        export_timeout: Duration::from_secs(5),
    };
    let _first = rskit_observability::init_tracer(&cfg).unwrap();
    let _second = rskit_observability::init_tracer(&cfg).unwrap();
}

#[tokio::test]
#[ignore = "requires OTLP collector"]
async fn init_metrics_with_endpoint() {
    let cfg = MetricsConfig {
        service_name: "test-svc".into(),
        export_interval: Duration::from_secs(15),
        otlp_endpoint: Some("http://localhost:4317".into()),
    };
    let handle = rskit_observability::init_metrics(&cfg).unwrap();
    let counter = handle.counter("test_counter", "a test counter");
    counter.add(1, &[]);
}
