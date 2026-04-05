use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use rskit_observability::{
    ComponentHealth, HealthStatus, MetricsConfig, MetricsHandle, OperationContext, ServiceHealth,
    TracingConfig,
};

// ── OperationContext: new ───────────────────────────────────────────────────

#[test]
fn operation_context_new_fields() {
    let ctx = OperationContext::new("my-svc", "create-user", "req-123", "user-456");
    assert_eq!(ctx.service_name, "my-svc");
    assert_eq!(ctx.operation_name, "create-user");
    assert_eq!(ctx.request_id, "req-123");
    assert_eq!(ctx.user_id, "user-456");
}

#[test]
fn operation_context_new_empty_strings() {
    let ctx = OperationContext::new("", "", "", "");
    assert_eq!(ctx.service_name, "");
    assert_eq!(ctx.operation_name, "");
    assert_eq!(ctx.request_id, "");
    assert_eq!(ctx.user_id, "");
}

#[test]
fn operation_context_accepts_string_and_str() {
    let svc = String::from("svc");
    let ctx = OperationContext::new(svc, "op", "req", "user");
    assert_eq!(ctx.service_name, "svc");
}

// ── OperationContext: elapsed ───────────────────────────────────────────────

#[test]
fn operation_context_elapsed_positive() {
    let ctx = OperationContext::new("svc", "op", "req", "user");
    std::thread::sleep(Duration::from_millis(10));
    let elapsed = ctx.elapsed();
    assert!(elapsed >= Duration::from_millis(5));
}

#[test]
fn operation_context_elapsed_grows() {
    let ctx = OperationContext::new("svc", "op", "req", "user");
    let t1 = ctx.elapsed();
    std::thread::sleep(Duration::from_millis(10));
    let t2 = ctx.elapsed();
    assert!(t2 > t1);
}

// ── OperationContext: start_span ────────────────────────────────────────────

#[test]
fn operation_context_start_span_does_not_panic() {
    let ctx = OperationContext::new("svc", "op", "req-1", "user-1");
    let _span = ctx.start_span("sub-operation");
}

#[test]
fn operation_context_multiple_spans() {
    let ctx = OperationContext::new("svc", "op", "req-1", "user-1");
    let _s1 = ctx.start_span("step-1");
    let _s2 = ctx.start_span("step-2");
    let _s3 = ctx.start_span("step-3");
}

// ── OperationContext: end_operation ─────────────────────────────────────────

#[test]
fn operation_context_end_operation_ok() {
    let ctx = OperationContext::new("svc", "op", "req-1", "user-1");
    ctx.end_operation("ok", None);
}

#[test]
fn operation_context_end_operation_with_error() {
    let ctx = OperationContext::new("svc", "op", "req-1", "user-1");
    let err = rskit_errors::AppError::new(rskit_errors::ErrorCode::Internal, "something failed");
    ctx.end_operation("error", Some(&err));
}

// ── OperationContext: with_metrics ──────────────────────────────────────────

#[test]
fn operation_context_with_metrics_no_endpoint() {
    let cfg = MetricsConfig {
        service_name: "test-svc".into(),
        export_interval: Duration::from_secs(15),
        otlp_endpoint: None,
    };
    let handle = rskit_observability::init_metrics(&cfg).unwrap();
    let metrics = Arc::new(handle);

    let ctx = OperationContext::new("svc", "op", "req", "user").with_metrics(metrics);
    ctx.end_operation("ok", None);
}

#[test]
fn operation_context_without_metrics() {
    let ctx = OperationContext::new("svc", "op", "req", "user");
    // Should not panic even without metrics
    ctx.end_operation("ok", None);
}

// ── ServiceHealth: new ─────────────────────────────────────────────────────

#[test]
fn service_health_new() {
    let sh = ServiceHealth::new("my-service", "1.2.3");
    assert_eq!(sh.service(), "my-service");
    assert_eq!(sh.version(), "1.2.3");
}

#[test]
fn service_health_empty_version() {
    let sh = ServiceHealth::new("svc", "");
    assert_eq!(sh.version(), "");
}

// ── ServiceHealth: register ─────────────────────────────────────────────────

#[test]
fn service_health_register_single() {
    let sh = ServiceHealth::new("svc", "1.0");
    sh.register("db");
    let status = sh.status();
    assert!(status.contains_key("db"));
    assert_eq!(status["db"].status, HealthStatus::Healthy);
}

#[test]
fn service_health_register_multiple() {
    let sh = ServiceHealth::new("svc", "1.0");
    sh.register("db");
    sh.register("cache");
    sh.register("queue");
    assert_eq!(sh.status().len(), 3);
}

#[test]
fn service_health_register_initially_healthy() {
    let sh = ServiceHealth::new("svc", "1.0");
    sh.register("component");
    assert!(sh.is_healthy());
    assert_eq!(sh.overall_status(), HealthStatus::Healthy);
}

// ── ServiceHealth: update ───────────────────────────────────────────────────

#[test]
fn service_health_update_to_degraded() {
    let sh = ServiceHealth::new("svc", "1.0");
    sh.register("db");
    sh.update("db", HealthStatus::Degraded, Some("slow queries".into()));
    let status = sh.status();
    assert_eq!(status["db"].status, HealthStatus::Degraded);
    assert_eq!(status["db"].message.as_deref(), Some("slow queries"));
}

#[test]
fn service_health_update_to_unhealthy() {
    let sh = ServiceHealth::new("svc", "1.0");
    sh.register("cache");
    sh.update(
        "cache",
        HealthStatus::Unhealthy,
        Some("connection refused".into()),
    );
    assert!(!sh.is_healthy());
}

#[test]
fn service_health_update_back_to_healthy() {
    let sh = ServiceHealth::new("svc", "1.0");
    sh.register("db");
    sh.update("db", HealthStatus::Unhealthy, Some("down".into()));
    sh.update("db", HealthStatus::Healthy, Some("recovered".into()));
    assert!(sh.is_healthy());
}

#[test]
fn service_health_update_with_none_message() {
    let sh = ServiceHealth::new("svc", "1.0");
    sh.register("db");
    sh.update("db", HealthStatus::Degraded, None);
    assert!(sh.status()["db"].message.is_none());
}

// ── ServiceHealth: is_healthy ───────────────────────────────────────────────

#[test]
fn service_health_all_healthy() {
    let sh = ServiceHealth::new("svc", "1.0");
    sh.register("a");
    sh.register("b");
    assert!(sh.is_healthy());
}

#[test]
fn service_health_one_degraded_not_healthy() {
    let sh = ServiceHealth::new("svc", "1.0");
    sh.register("a");
    sh.register("b");
    sh.update("b", HealthStatus::Degraded, None);
    assert!(!sh.is_healthy());
}

#[test]
fn service_health_empty_is_healthy() {
    let sh = ServiceHealth::new("svc", "1.0");
    assert!(sh.is_healthy());
}

// ── ServiceHealth: overall_status ───────────────────────────────────────────

#[test]
fn service_health_overall_all_healthy() {
    let sh = ServiceHealth::new("svc", "1.0");
    sh.register("a");
    sh.register("b");
    assert_eq!(sh.overall_status(), HealthStatus::Healthy);
}

#[test]
fn service_health_overall_one_degraded() {
    let sh = ServiceHealth::new("svc", "1.0");
    sh.register("a");
    sh.update("a", HealthStatus::Degraded, None);
    assert_eq!(sh.overall_status(), HealthStatus::Degraded);
}

#[test]
fn service_health_overall_one_unhealthy() {
    let sh = ServiceHealth::new("svc", "1.0");
    sh.register("a");
    sh.register("b");
    sh.update("a", HealthStatus::Unhealthy, None);
    assert_eq!(sh.overall_status(), HealthStatus::Unhealthy);
}

#[test]
fn service_health_overall_unhealthy_trumps_degraded() {
    let sh = ServiceHealth::new("svc", "1.0");
    sh.register("a");
    sh.register("b");
    sh.update("a", HealthStatus::Degraded, None);
    sh.update("b", HealthStatus::Unhealthy, None);
    assert_eq!(sh.overall_status(), HealthStatus::Unhealthy);
}

#[test]
fn service_health_overall_empty_returns_healthy() {
    let sh = ServiceHealth::new("svc", "1.0");
    assert_eq!(sh.overall_status(), HealthStatus::Healthy);
}

// ── ServiceHealth: mixed statuses ───────────────────────────────────────────

#[test]
fn service_health_mixed_healthy_degraded() {
    let sh = ServiceHealth::new("svc", "1.0");
    sh.register("db");
    sh.register("cache");
    sh.register("queue");
    sh.update("cache", HealthStatus::Degraded, Some("slow".into()));
    assert_eq!(sh.overall_status(), HealthStatus::Degraded);
    assert!(!sh.is_healthy());
}

#[test]
fn service_health_mixed_all_three() {
    let sh = ServiceHealth::new("svc", "1.0");
    sh.register("db");
    sh.register("cache");
    sh.register("queue");
    sh.update("cache", HealthStatus::Degraded, None);
    sh.update("queue", HealthStatus::Unhealthy, None);
    assert_eq!(sh.overall_status(), HealthStatus::Unhealthy);
}

// ── ServiceHealth: concurrent updates (RwLock) ─────────────────────────────

#[test]
fn service_health_concurrent_updates() {
    let sh = ServiceHealth::new("svc", "1.0");
    let handles: Vec<_> = (0..20)
        .map(|i| {
            let sh = sh.clone();
            std::thread::spawn(move || {
                let name = format!("comp-{i}");
                sh.register(&name);
                sh.update(&name, HealthStatus::Degraded, Some("test".into()));
                sh.update(&name, HealthStatus::Healthy, None);
                let _ = sh.is_healthy();
                let _ = sh.overall_status();
                let _ = sh.status();
            })
        })
        .collect();

    for h in handles {
        h.join().unwrap();
    }
    assert_eq!(sh.status().len(), 20);
}

#[test]
fn service_health_clone_is_shared() {
    let sh = ServiceHealth::new("svc", "1.0");
    let sh2 = sh.clone();
    sh.register("db");
    assert!(sh2.status().contains_key("db"));
}

// ── ComponentHealth ─────────────────────────────────────────────────────────

#[test]
fn component_health_construction() {
    let ch = ComponentHealth {
        name: "db".into(),
        status: HealthStatus::Healthy,
        message: None,
    };
    assert_eq!(ch.name, "db");
    assert_eq!(ch.status, HealthStatus::Healthy);
    assert!(ch.message.is_none());
}

#[test]
fn component_health_with_message() {
    let ch = ComponentHealth {
        name: "cache".into(),
        status: HealthStatus::Degraded,
        message: Some("slow".into()),
    };
    assert_eq!(ch.message.as_deref(), Some("slow"));
}

#[test]
fn component_health_clone() {
    let ch = ComponentHealth {
        name: "db".into(),
        status: HealthStatus::Healthy,
        message: Some("ok".into()),
    };
    let ch2 = ch.clone();
    assert_eq!(ch.name, ch2.name);
    assert_eq!(ch.status, ch2.status);
}

#[test]
fn component_health_debug() {
    let ch = ComponentHealth {
        name: "db".into(),
        status: HealthStatus::Healthy,
        message: None,
    };
    let debug = format!("{:?}", ch);
    assert!(debug.contains("db"));
    assert!(debug.contains("Healthy"));
}

// ── HealthStatus enum ───────────────────────────────────────────────────────

#[test]
fn health_status_equality() {
    assert_eq!(HealthStatus::Healthy, HealthStatus::Healthy);
    assert_eq!(HealthStatus::Degraded, HealthStatus::Degraded);
    assert_eq!(HealthStatus::Unhealthy, HealthStatus::Unhealthy);
}

#[test]
fn health_status_inequality() {
    assert_ne!(HealthStatus::Healthy, HealthStatus::Degraded);
    assert_ne!(HealthStatus::Healthy, HealthStatus::Unhealthy);
    assert_ne!(HealthStatus::Degraded, HealthStatus::Unhealthy);
}

#[test]
fn health_status_clone_copy() {
    let s = HealthStatus::Healthy;
    let s2 = s; // Copy
    let s3 = s.clone();
    assert_eq!(s, s2);
    assert_eq!(s, s3);
}

#[test]
fn health_status_debug() {
    let debug = format!("{:?}", HealthStatus::Degraded);
    assert_eq!(debug, "Degraded");
}

#[test]
fn health_status_serialization() {
    let json = serde_json::to_string(&HealthStatus::Healthy).unwrap();
    assert_eq!(json, "\"healthy\"");
    let json = serde_json::to_string(&HealthStatus::Degraded).unwrap();
    assert_eq!(json, "\"degraded\"");
    let json = serde_json::to_string(&HealthStatus::Unhealthy).unwrap();
    assert_eq!(json, "\"unhealthy\"");
}

#[test]
fn health_status_deserialization() {
    let s: HealthStatus = serde_json::from_str("\"healthy\"").unwrap();
    assert_eq!(s, HealthStatus::Healthy);
    let s: HealthStatus = serde_json::from_str("\"degraded\"").unwrap();
    assert_eq!(s, HealthStatus::Degraded);
    let s: HealthStatus = serde_json::from_str("\"unhealthy\"").unwrap();
    assert_eq!(s, HealthStatus::Unhealthy);
}

// ── MetricsHandle instruments ───────────────────────────────────────────────

fn make_metrics_handle() -> MetricsHandle {
    let cfg = MetricsConfig {
        service_name: "test-svc".into(),
        export_interval: Duration::from_secs(15),
        otlp_endpoint: None,
    };
    rskit_observability::init_metrics(&cfg).unwrap()
}

#[test]
fn metrics_handle_counter() {
    let handle = make_metrics_handle();
    let counter = handle.counter("test.requests", "Total requests");
    counter.add(1, &[]);
    counter.add(5, &[]);
}

#[test]
fn metrics_handle_histogram() {
    let handle = make_metrics_handle();
    let hist = handle.histogram("test.latency", "Request latency");
    hist.record(0.123, &[]);
    hist.record(1.5, &[]);
}

#[test]
fn metrics_handle_gauge() {
    let handle = make_metrics_handle();
    let gauge = handle.gauge("test.connections", "Active connections");
    gauge.record(10.0, &[]);
    gauge.record(5.0, &[]);
}

#[test]
fn metrics_handle_up_down_counter() {
    let handle = make_metrics_handle();
    let udc = handle.up_down_counter("test.active", "Active items");
    udc.add(1, &[]);
    udc.add(-1, &[]);
}

#[test]
fn metrics_handle_multiple_instruments() {
    let handle = make_metrics_handle();
    let _c = handle.counter("c", "counter");
    let _h = handle.histogram("h", "histogram");
    let _g = handle.gauge("g", "gauge");
    let _u = handle.up_down_counter("u", "up_down_counter");
}

// ── MetricsConfig ───────────────────────────────────────────────────────────

#[test]
fn metrics_config_with_endpoint() {
    let cfg = MetricsConfig {
        service_name: "svc".into(),
        export_interval: Duration::from_secs(30),
        otlp_endpoint: Some("http://localhost:4317".into()),
    };
    assert_eq!(cfg.otlp_endpoint.as_deref(), Some("http://localhost:4317"));
}

#[test]
fn metrics_config_without_endpoint() {
    let cfg = MetricsConfig {
        service_name: "svc".into(),
        export_interval: Duration::from_secs(60),
        otlp_endpoint: None,
    };
    assert!(cfg.otlp_endpoint.is_none());
}

#[test]
fn metrics_config_debug() {
    let cfg = MetricsConfig {
        service_name: "svc".into(),
        export_interval: Duration::from_secs(15),
        otlp_endpoint: None,
    };
    let debug = format!("{:?}", cfg);
    assert!(debug.contains("svc"));
}

#[test]
fn init_metrics_without_endpoint_succeeds() {
    let cfg = MetricsConfig {
        service_name: "test".into(),
        export_interval: Duration::from_secs(15),
        otlp_endpoint: None,
    };
    let result = rskit_observability::init_metrics(&cfg);
    assert!(result.is_ok());
}

// ── TracingConfig ───────────────────────────────────────────────────────────

#[test]
fn tracing_config_sampler_always_on() {
    let cfg = TracingConfig {
        service_name: "svc".into(),
        endpoint: "http://localhost:4317".into(),
        sample_rate: 1.0,
        export_timeout: Duration::from_secs(5),
    };
    assert!((cfg.sample_rate - 1.0).abs() < f64::EPSILON);
}

#[test]
fn tracing_config_sampler_always_off() {
    let cfg = TracingConfig {
        service_name: "svc".into(),
        endpoint: "http://localhost:4317".into(),
        sample_rate: 0.0,
        export_timeout: Duration::from_secs(5),
    };
    assert!((cfg.sample_rate - 0.0).abs() < f64::EPSILON);
}

#[test]
fn tracing_config_sampler_ratio() {
    let cfg = TracingConfig {
        service_name: "svc".into(),
        endpoint: "http://localhost:4317".into(),
        sample_rate: 0.5,
        export_timeout: Duration::from_secs(5),
    };
    assert!((cfg.sample_rate - 0.5).abs() < f64::EPSILON);
}

#[test]
fn tracing_config_debug() {
    let cfg = TracingConfig {
        service_name: "svc".into(),
        endpoint: "http://localhost:4317".into(),
        sample_rate: 1.0,
        export_timeout: Duration::from_secs(5),
    };
    let debug = format!("{:?}", cfg);
    assert!(debug.contains("svc"));
}

#[test]
fn tracing_config_clone() {
    let cfg = TracingConfig {
        service_name: "svc".into(),
        endpoint: "http://localhost:4317".into(),
        sample_rate: 1.0,
        export_timeout: Duration::from_secs(5),
    };
    let cfg2 = cfg.clone();
    assert_eq!(cfg.service_name, cfg2.service_name);
    assert_eq!(cfg.endpoint, cfg2.endpoint);
}

// ── Propagation ─────────────────────────────────────────────────────────────

#[test]
fn inject_into_empty_headers_no_panic() {
    let mut headers = http::HeaderMap::new();
    rskit_observability::inject_trace_context(&mut headers);
}

#[test]
fn extract_from_empty_headers_returns_context() {
    let headers = http::HeaderMap::new();
    let _ctx = rskit_observability::extract_trace_context(&headers);
}

#[test]
fn extract_with_valid_traceparent() {
    let mut headers = http::HeaderMap::new();
    headers.insert(
        "traceparent",
        "00-4bf92f3577b34da6a3ce929d0e0e4736-00f067aa0ba902b7-01"
            .parse()
            .unwrap(),
    );
    let _ctx = rskit_observability::extract_trace_context(&headers);
}

#[test]
fn inject_then_extract_roundtrip() {
    let mut headers = http::HeaderMap::new();
    rskit_observability::inject_trace_context(&mut headers);
    let _ctx = rskit_observability::extract_trace_context(&headers);
}

// ── Error handling paths ────────────────────────────────────────────────────

#[test]
fn operation_context_end_with_various_errors() {
    let ctx = OperationContext::new("svc", "op", "req", "user");

    let err1 = rskit_errors::AppError::new(rskit_errors::ErrorCode::NotFound, "not found");
    ctx.end_operation("error", Some(&err1));
}

#[test]
fn operation_context_end_with_internal_error() {
    let ctx = OperationContext::new("svc", "op", "req", "user");
    let err = rskit_errors::AppError::new(rskit_errors::ErrorCode::Internal, "internal failure");
    ctx.end_operation("error", Some(&err));
}

// ── Concurrent OperationContext from multiple threads ────────────────────────

#[test]
fn concurrent_operation_contexts() {
    let handles: Vec<_> = (0..20)
        .map(|i| {
            std::thread::spawn(move || {
                let ctx = OperationContext::new(
                    "svc",
                    format!("op-{i}"),
                    format!("req-{i}"),
                    format!("user-{i}"),
                );
                let _span = ctx.start_span(&format!("span-{i}"));
                std::thread::sleep(Duration::from_millis(1));
                ctx.end_operation("ok", None);
                assert!(ctx.elapsed() >= Duration::from_millis(1));
            })
        })
        .collect();

    for h in handles {
        h.join().unwrap();
    }
}

#[test]
fn concurrent_operation_contexts_with_metrics() {
    let cfg = MetricsConfig {
        service_name: "test-svc".into(),
        export_interval: Duration::from_secs(15),
        otlp_endpoint: None,
    };
    let handle = Arc::new(rskit_observability::init_metrics(&cfg).unwrap());

    let handles: Vec<_> = (0..10)
        .map(|i| {
            let metrics = Arc::clone(&handle);
            std::thread::spawn(move || {
                let ctx = OperationContext::new("svc", format!("op-{i}"), format!("req-{i}"), "")
                    .with_metrics(metrics);
                ctx.end_operation("ok", None);
            })
        })
        .collect();

    for h in handles {
        h.join().unwrap();
    }
}
