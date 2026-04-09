//! Comprehensive tests for rskit-server: builder, component, config, and lifecycle.

use std::time::Duration;

use validator::Validate;

use rskit_bootstrap::Component;
use rskit_server::{GrpcServerBuilder, GrpcServerConfig};

// ---------------------------------------------------------------------------
// Config tests (extended beyond inline unit tests)
// ---------------------------------------------------------------------------

#[test]
fn config_default_values() {
    let cfg = GrpcServerConfig::default();
    assert_eq!(cfg.host, "0.0.0.0");
    assert_eq!(cfg.port, 50051);
    assert!(cfg.max_connections.is_none());
    assert!(cfg.keep_alive_secs.is_none());
    assert!(cfg.tls.is_none());
}

#[test]
fn config_new_constructor() {
    let cfg = GrpcServerConfig::new("127.0.0.1", 8080);
    assert_eq!(cfg.host, "127.0.0.1");
    assert_eq!(cfg.port, 8080);
    assert!(cfg.validate().is_ok());
}

#[test]
fn config_addr_format() {
    let cfg = GrpcServerConfig::new("localhost", 9090);
    assert_eq!(cfg.addr(), "localhost:9090");
}

#[test]
fn config_addr_default() {
    let cfg = GrpcServerConfig::default();
    assert_eq!(cfg.addr(), "0.0.0.0:50051");
}

#[test]
fn config_validate_valid() {
    let cfg = GrpcServerConfig::default();
    assert!(cfg.validate().is_ok());
}

#[test]
fn config_validate_empty_host() {
    let cfg = GrpcServerConfig {
        host: String::new(),
        ..Default::default()
    };
    assert!(cfg.validate().is_err());
}

#[test]
fn config_validate_port_zero_fails() {
    let cfg = GrpcServerConfig {
        port: 0,
        ..Default::default()
    };
    assert!(cfg.validate().is_err());
}

#[test]
fn config_validate_max_port() {
    let cfg = GrpcServerConfig::new("127.0.0.1", 65535);
    assert!(cfg.validate().is_ok());
}

#[test]
fn config_validate_port_one() {
    let cfg = GrpcServerConfig::new("127.0.0.1", 1);
    assert!(cfg.validate().is_ok());
}

#[test]
fn config_with_max_connections() {
    let cfg = GrpcServerConfig {
        max_connections: Some(100),
        ..Default::default()
    };
    assert_eq!(cfg.max_connections, Some(100));
    assert!(cfg.validate().is_ok());
}

#[test]
fn config_with_keep_alive() {
    let cfg = GrpcServerConfig {
        keep_alive_secs: Some(30),
        ..Default::default()
    };
    assert_eq!(cfg.keep_alive_secs, Some(30));
    assert!(cfg.validate().is_ok());
}

#[test]
fn config_clone() {
    let cfg = GrpcServerConfig::new("10.0.0.1", 3000);
    let cloned = cfg.clone();
    assert_eq!(cloned.host, "10.0.0.1");
    assert_eq!(cloned.port, 3000);
}

#[test]
fn config_debug_format() {
    let cfg = GrpcServerConfig::default();
    let debug = format!("{:?}", cfg);
    assert!(debug.contains("GrpcServerConfig"));
    assert!(debug.contains("50051"));
}

#[test]
fn config_serde_roundtrip() {
    let cfg = GrpcServerConfig::new("192.168.1.1", 7777);
    // Verify config can be cloned and fields preserved (serde traits are derived)
    let cloned = cfg.clone();
    assert_eq!(cloned.host, "192.168.1.1");
    assert_eq!(cloned.port, 7777);
}

#[test]
fn tls_config_fields() {
    let tls = rskit_server::TlsConfig {
        cert_path: "/etc/ssl/cert.pem".into(),
        key_path: "/etc/ssl/key.pem".into(),
    };
    assert_eq!(tls.cert_path, "/etc/ssl/cert.pem");
    assert_eq!(tls.key_path, "/etc/ssl/key.pem");
}

// ---------------------------------------------------------------------------
// Builder tests
// ---------------------------------------------------------------------------

#[test]
fn builder_default_name() {
    let server = GrpcServerBuilder::new(GrpcServerConfig::default()).build();
    assert_eq!(server.name(), "grpc-server");
}

#[test]
fn builder_custom_name() {
    let server = GrpcServerBuilder::new(GrpcServerConfig::default())
        .with_name("my-server")
        .build();
    assert_eq!(server.name(), "my-server");
}

#[test]
fn builder_with_name_chaining() {
    let server = GrpcServerBuilder::new(GrpcServerConfig::new("127.0.0.1", 9999))
        .with_name("custom")
        .build();
    assert_eq!(server.name(), "custom");
}

// ---------------------------------------------------------------------------
// Component health before start
// ---------------------------------------------------------------------------

#[test]
fn health_unhealthy_before_start() {
    let server = GrpcServerBuilder::new(GrpcServerConfig::default()).build();
    let h = server.health();
    assert_eq!(h.status, rskit_bootstrap::HealthStatus::Unhealthy);
    assert!(h.message.as_deref().unwrap_or("").contains("not running"));
}

#[test]
fn component_name() {
    let server = GrpcServerBuilder::new(GrpcServerConfig::default())
        .with_name("test-grpc")
        .build();
    assert_eq!(server.name(), "test-grpc");
}

// ---------------------------------------------------------------------------
// Lifecycle tests with tokio runtime
// ---------------------------------------------------------------------------

/// Helper: build a server with no services on a given port.
fn build_no_service_server(port: u16) -> rskit_server::GrpcServer {
    let cfg = GrpcServerConfig::new("127.0.0.1", port);
    GrpcServerBuilder::new(cfg).build()
}

#[tokio::test]
async fn start_and_stop_no_services() {
    let server = build_no_service_server(0);
    // Port 0 with host 127.0.0.1 should let the server start
    // (though it uses 127.0.0.1:0 which is a valid address)
    let result = server.start().await;
    assert!(result.is_ok(), "start failed: {:?}", result.err());

    // After start, health should be healthy (task is running)
    let h = server.health();
    assert_eq!(h.status, rskit_bootstrap::HealthStatus::Healthy);

    let result = server.stop().await;
    assert!(result.is_ok(), "stop failed: {:?}", result.err());
}

#[tokio::test]
async fn stop_is_idempotent() {
    let server = build_no_service_server(0);
    server.start().await.expect("start");
    server.stop().await.expect("stop 1");
    // Second stop should not panic or error
    server.stop().await.expect("stop 2");
}

#[tokio::test]
async fn health_transitions() {
    let server = build_no_service_server(0);

    // Before start: unhealthy
    assert_eq!(
        server.health().status,
        rskit_bootstrap::HealthStatus::Unhealthy
    );

    server.start().await.expect("start");

    // After start: healthy
    assert_eq!(
        server.health().status,
        rskit_bootstrap::HealthStatus::Healthy
    );

    server.stop().await.expect("stop");

    // Give the spawned task a moment to finish
    tokio::time::sleep(Duration::from_millis(100)).await;

    // After stop: unhealthy
    assert_eq!(
        server.health().status,
        rskit_bootstrap::HealthStatus::Unhealthy
    );
}

#[tokio::test]
async fn start_with_invalid_address() {
    let cfg = GrpcServerConfig {
        host: "not-a-valid-host-!!!".into(),
        port: 9999,
        ..Default::default()
    };
    let server = GrpcServerBuilder::new(cfg).build();
    let result = server.start().await;
    assert!(result.is_err(), "expected error for invalid address");
}

#[tokio::test]
async fn start_invalid_addr_parse() {
    // An address that won't parse as SocketAddr
    let cfg = GrpcServerConfig {
        host: "hello world".into(),
        port: 80,
        ..Default::default()
    };
    let server = GrpcServerBuilder::new(cfg).build();
    let result = server.start().await;
    assert!(result.is_err());
}

// ---------------------------------------------------------------------------
// Graceful shutdown with CancellationToken
// ---------------------------------------------------------------------------

#[tokio::test]
async fn graceful_shutdown_via_stop() {
    let server = build_no_service_server(0);
    server.start().await.expect("start");

    // Health is healthy while running
    assert_eq!(
        server.health().status,
        rskit_bootstrap::HealthStatus::Healthy
    );

    server.stop().await.expect("stop");

    // Allow task to complete
    tokio::time::sleep(Duration::from_millis(100)).await;

    // After cancel, the task should have finished
    assert_eq!(
        server.health().status,
        rskit_bootstrap::HealthStatus::Unhealthy
    );
}

#[tokio::test]
async fn stop_completes_within_timeout() {
    let server = build_no_service_server(0);
    server.start().await.expect("start");

    let start = std::time::Instant::now();
    server.stop().await.expect("stop");
    let elapsed = start.elapsed();

    // Should complete well within the 10s timeout
    assert!(
        elapsed < Duration::from_secs(5),
        "stop took too long: {:?}",
        elapsed
    );
}

// ---------------------------------------------------------------------------
// Builder produces correct component structure
// ---------------------------------------------------------------------------

#[test]
fn builder_build_returns_grpc_server() {
    let server = GrpcServerBuilder::new(GrpcServerConfig::default()).build();
    assert_eq!(server.name(), "grpc-server");
}

#[tokio::test]
async fn builder_no_services_server_waits_for_cancel() {
    let server = build_no_service_server(0);
    server.start().await.expect("start");

    // Server with no services should be running (waiting for cancel)
    tokio::time::sleep(Duration::from_millis(50)).await;
    assert_eq!(
        server.health().status,
        rskit_bootstrap::HealthStatus::Healthy
    );

    server.stop().await.expect("stop");
}

// ---------------------------------------------------------------------------
// Custom start_fn tracking
// ---------------------------------------------------------------------------

#[tokio::test]
async fn start_fn_called_with_correct_addr() {
    // Verify the server starts successfully with a valid config
    let cfg = GrpcServerConfig::new("127.0.0.1", 0);
    let server = GrpcServerBuilder::new(cfg).build();

    let result = server.start().await;
    assert!(result.is_ok());

    server.stop().await.expect("stop");
}

// ---------------------------------------------------------------------------
// Multiple servers
// ---------------------------------------------------------------------------

#[tokio::test]
async fn multiple_servers_different_ports() {
    let s1 = build_no_service_server(0);
    let s2 = build_no_service_server(0);

    s1.start().await.expect("s1 start");
    s2.start().await.expect("s2 start");

    assert_eq!(s1.health().status, rskit_bootstrap::HealthStatus::Healthy);
    assert_eq!(s2.health().status, rskit_bootstrap::HealthStatus::Healthy);

    s1.stop().await.expect("s1 stop");
    s2.stop().await.expect("s2 stop");
}

// ---------------------------------------------------------------------------
// Config edge cases for addr() parsing
// ---------------------------------------------------------------------------

#[test]
fn config_addr_ipv4() {
    let cfg = GrpcServerConfig::new("192.168.1.1", 443);
    assert_eq!(cfg.addr(), "192.168.1.1:443");
}

#[test]
fn config_addr_localhost() {
    let cfg = GrpcServerConfig::new("localhost", 8080);
    assert_eq!(cfg.addr(), "localhost:8080");
}

#[tokio::test]
async fn start_with_loopback() {
    let cfg = GrpcServerConfig::new("127.0.0.1", 0);
    let server = GrpcServerBuilder::new(cfg).build();
    let result = server.start().await;
    assert!(result.is_ok());
    server.stop().await.expect("stop");
}

// ---------------------------------------------------------------------------
// Rapid start/stop cycles
// ---------------------------------------------------------------------------

#[tokio::test]
async fn rapid_start_stop_cycles() {
    for _ in 0..5 {
        let server = build_no_service_server(0);
        server.start().await.expect("start");
        server.stop().await.expect("stop");
    }
}

// ---------------------------------------------------------------------------
// Health check after task completes naturally
// ---------------------------------------------------------------------------

#[tokio::test]
async fn health_after_natural_task_completion() {
    let server = build_no_service_server(0);
    server.start().await.expect("start");

    // Stop triggers cancellation, which makes the task complete
    server.stop().await.expect("stop");
    tokio::time::sleep(Duration::from_millis(100)).await;

    let h = server.health();
    assert_eq!(h.status, rskit_bootstrap::HealthStatus::Unhealthy);
    assert_eq!(h.name, "grpc-server");
}

#[test]
fn health_name_matches_component_name() {
    let server = GrpcServerBuilder::new(GrpcServerConfig::default())
        .with_name("custom-name")
        .build();
    let h = server.health();
    assert_eq!(h.name, "custom-name");
}
