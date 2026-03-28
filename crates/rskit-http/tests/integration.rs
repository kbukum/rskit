use std::time::Duration;

use rskit_errors::{AppError, ErrorCode};
use rskit_http::{CorsConfig, HttpError, HttpServerConfig};

// ── HttpServerConfig ────────────────────────────────────────────────────────

#[test]
fn http_server_config_defaults() {
    let cfg = HttpServerConfig::default();
    assert_eq!(cfg.host, "0.0.0.0");
    assert_eq!(cfg.port, 8080);
    assert_eq!(cfg.read_timeout, Duration::from_secs(30));
    assert_eq!(cfg.write_timeout, Duration::from_secs(30));
    assert_eq!(cfg.idle_timeout, Duration::from_secs(60));
    assert!(cfg.enable_h2c);
    assert!(cfg.cors.is_none());
}

#[test]
fn http_server_config_bind_addr() {
    let cfg = HttpServerConfig::default();
    assert_eq!(cfg.bind_addr(), "0.0.0.0:8080");
}

#[test]
fn http_server_config_custom_bind_addr() {
    let cfg = HttpServerConfig {
        host: "127.0.0.1".into(),
        port: 3000,
        ..Default::default()
    };
    assert_eq!(cfg.bind_addr(), "127.0.0.1:3000");
}

// ── CorsConfig ──────────────────────────────────────────────────────────────

#[test]
fn cors_config_construction() {
    let cors = CorsConfig {
        allowed_origins: vec!["https://example.com".into()],
        allowed_methods: vec!["GET".into(), "POST".into()],
        allowed_headers: vec!["Content-Type".into()],
        allow_credentials: true,
        max_age: Duration::from_secs(3600),
    };
    assert_eq!(cors.allowed_origins.len(), 1);
    assert_eq!(cors.allowed_methods.len(), 2);
    assert!(cors.allow_credentials);
    assert_eq!(cors.max_age, Duration::from_secs(3600));
}

// ── HttpError ───────────────────────────────────────────────────────────────

#[test]
fn http_error_from_app_error() {
    let app_err = AppError::not_found("user", Some("42"));
    let http_err: HttpError = app_err.into();
    assert_eq!(http_err.0.code, ErrorCode::NotFound);
    assert_eq!(http_err.0.http_status.as_u16(), 404);
}

#[test]
fn http_error_from_internal_error() {
    let app_err = AppError::new(ErrorCode::Internal, "something broke");
    let http_err = HttpError::from(app_err);
    assert_eq!(http_err.0.code, ErrorCode::Internal);
}

// ── Extractors ──────────────────────────────────────────────────────────────

#[test]
fn request_id_struct_holds_string() {
    let id = rskit_http::RequestId("abc-123".into());
    assert_eq!(id.0, "abc-123");
}

#[test]
fn correlation_id_struct_holds_string() {
    let id = rskit_http::CorrelationId("corr-456".into());
    assert_eq!(id.0, "corr-456");
}

// ── Server (requires bind) ──────────────────────────────────────────────────

#[tokio::test]
#[ignore = "requires free port and tokio runtime for server lifecycle"]
async fn http_server_starts_and_stops() {
    use rskit_http::HttpServerBuilder;
    use tokio_util::sync::CancellationToken;

    let cfg = HttpServerConfig {
        port: 0, // ephemeral port
        ..Default::default()
    };
    let cancel = CancellationToken::new();
    let server = HttpServerBuilder::new(cfg, cancel.clone())
        .with_request_id()
        .build();

    use rskit_bootstrap::Component;
    server.start().await.unwrap();
    cancel.cancel();
    server.stop().await.unwrap();
}
