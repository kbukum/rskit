use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use axum::{
    Router,
    body::{Body, to_bytes},
    http::Request,
    routing::{get, post},
};
use rskit_bootstrap::{Component, Health, Registry};
use rskit_security::TlsVersion;
use rskit_server::{
    CorsPolicy, HTTP_BASELINE_LAYER_ORDER, HTTP_INTERCEPTOR_ORDER, HttpMiddlewareStack,
    HttpServerBuilder, HttpServerConfig, HttpTlsConfig, SecurityHeadersConfig, TransportSecurity,
    health_router, healthz_router,
};
use rskit_validation::Validate;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use tokio_util::sync::CancellationToken;
use tower::ServiceExt;

async fn raw_http_request(port: u16, request: &str) -> String {
    let deadline = tokio::time::Instant::now() + Duration::from_secs(2);
    let mut stream = loop {
        match TcpStream::connect(("127.0.0.1", port)).await {
            Ok(stream) => break stream,
            Err(error) if tokio::time::Instant::now() < deadline => {
                let _ = error;
                tokio::time::sleep(Duration::from_millis(10)).await;
            }
            Err(error) => panic!("server did not accept connections on {port}: {error}"),
        }
    };

    stream.write_all(request.as_bytes()).await.unwrap();
    let mut response = Vec::new();
    tokio::time::timeout(Duration::from_secs(2), stream.read_to_end(&mut response))
        .await
        .unwrap()
        .unwrap();
    String::from_utf8(response).unwrap()
}

fn loopback_http_config(port: u16) -> HttpServerConfig {
    HttpServerConfig {
        host: "127.0.0.1".to_string(),
        port,
        ..HttpServerConfig::default()
    }
}

fn assigned_port(server: &rskit_server::HttpServer) -> u16 {
    server
        .local_addr()
        .expect("server should expose bound local address after start")
        .port()
}

#[test]
fn http_config_defaults_match_transport_policy() {
    let cfg = HttpServerConfig::default();
    assert_eq!(cfg.bind_addr(), "0.0.0.0:8080");
    assert_eq!(cfg.read_timeout, Duration::from_secs(30));
    assert!(cfg.enable_h2c);
}

#[test]
fn interceptor_order_is_locked() {
    assert_eq!(
        HTTP_INTERCEPTOR_ORDER,
        ["tracing", "logging", "auth", "validation", "metrics"]
    );
    assert_eq!(
        HTTP_BASELINE_LAYER_ORDER,
        [
            "request_id",
            "cors",
            "security_headers",
            "body_limit",
            "timeout"
        ]
    );
}

#[test]
fn builder_accepts_ordered_middleware_stack() {
    let server = HttpServerBuilder::new(HttpServerConfig::default(), CancellationToken::new())
        .with_middleware_stack(HttpMiddlewareStack::new())
        .with_router(Router::new().route("/", get(|| async { "ok" })))
        .build()
        .unwrap();
    assert_eq!(server.bind_addr(), "0.0.0.0:8080");
}

#[tokio::test]
async fn health_routes_return_expected_statuses() {
    let ok_response = health_router(Arc::new(Registry::new()))
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(ok_response.status(), axum::http::StatusCode::OK);

    let healthz_response = healthz_router()
        .oneshot(
            Request::builder()
                .uri("/healthz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(healthz_response.status(), axum::http::StatusCode::OK);
}

#[derive(Debug)]
struct StaticHealthComponent {
    name: &'static str,
    health: Health,
}

#[async_trait]
impl Component for StaticHealthComponent {
    fn name(&self) -> &str {
        self.name
    }

    async fn start(&self) -> rskit_errors::AppResult<()> {
        Ok(())
    }

    async fn stop(&self) -> rskit_errors::AppResult<()> {
        Ok(())
    }

    fn health(&self) -> Health {
        self.health.clone()
    }
}

#[tokio::test]
async fn health_router_returns_unavailable_when_any_component_is_unhealthy() {
    let mut registry = Registry::new();
    registry.register(Arc::new(StaticHealthComponent {
        name: "database",
        health: Health::unhealthy("database", "connection refused"),
    }));

    let response = health_router(Arc::new(registry))
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(
        response.status(),
        axum::http::StatusCode::SERVICE_UNAVAILABLE
    );
    let body = to_bytes(response.into_body(), 4096).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json[0]["name"], "database");
}

#[tokio::test]
async fn healthz_router_returns_liveness_payload() {
    let response = healthz_router()
        .oneshot(
            Request::builder()
                .uri("/healthz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), axum::http::StatusCode::OK);
    let body = to_bytes(response.into_body(), 4096).await.unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["status"], "ok");
    assert!(json["version"].as_str().unwrap().contains('.'));
}

#[test]
fn cors_policy_is_exposed_from_server_crate() {
    let cfg = CorsPolicy {
        allowed_origins: vec!["https://example.com".to_string()],
        allowed_methods: vec!["GET".to_string()],
        allowed_headers: vec!["authorization".to_string()],
        allow_credentials: true,
        max_age: Duration::from_secs(60),
    };
    assert_eq!(cfg.allowed_origins[0], "https://example.com");
}

#[test]
fn http_config_validation_rejects_invalid_cors_policy() {
    let cfg = HttpServerConfig {
        cors: Some(CorsPolicy {
            allowed_origins: vec!["*".to_string()],
            ..CorsPolicy::default()
        }),
        ..HttpServerConfig::default()
    };

    assert!(cfg.validate().is_err());
}

#[test]
fn http_tls_requires_server_identity() {
    let cfg = HttpServerConfig {
        tls: Some(HttpTlsConfig::default()),
        ..HttpServerConfig::default()
    };

    assert!(cfg.validate().is_err());
}

#[test]
fn http_tls_rejects_client_only_options() {
    let cfg = HttpServerConfig {
        tls: Some(HttpTlsConfig {
            cert_file: Some("cert.pem".to_string()),
            key_file: Some("key.pem".to_string()),
            server_name: Some("example.com".to_string()),
            ..HttpTlsConfig::default()
        }),
        ..HttpServerConfig::default()
    };

    assert!(cfg.validate().is_err());
}

#[test]
fn http_tls_accepts_certificate_key_pair() {
    let cfg = HttpServerConfig {
        tls: Some(HttpTlsConfig {
            cert_file: Some("cert.pem".to_string()),
            key_file: Some("key.pem".to_string()),
            min_version: TlsVersion::Tls12,
            ..HttpTlsConfig::default()
        }),
        ..HttpServerConfig::default()
    };

    assert!(cfg.validate().is_ok());
}

#[test]
fn builder_validates_cors_and_security_header_configuration() {
    let invalid_cors = HttpServerConfig {
        cors: Some(CorsPolicy {
            allowed_origins: vec!["*".to_string()],
            ..CorsPolicy::default()
        }),
        ..HttpServerConfig::default()
    };

    assert!(
        HttpServerBuilder::new(invalid_cors, CancellationToken::new())
            .with_cors()
            .is_err()
    );

    let invalid_headers = SecurityHeadersConfig::default()
        .with_transport_security(TransportSecurity::AllowInsecureLocal)
        .with_content_security_policy(None)
        .with_permissions_policy(None)
        .with_referrer_policy(None)
        .with_frame_options(None)
        .with_content_type_options(None);

    assert!(
        HttpServerBuilder::new(HttpServerConfig::default(), CancellationToken::new())
            .with_security_headers_config(invalid_headers)
            .is_err()
    );
}

#[tokio::test]
async fn http_server_serves_merged_routes_with_security_headers() {
    let server = HttpServerBuilder::new(loopback_http_config(0), CancellationToken::new())
        .with_router(Router::new().route("/one", get(|| async { "one" })))
        .with_router(Router::new().route("/two", get(|| async { "two" })))
        .with_security_headers()
        .unwrap()
        .build()
        .unwrap();

    server.start().await.unwrap();
    let port = assigned_port(&server);
    let response = raw_http_request(
        port,
        "GET /two HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
    )
    .await;
    server.stop().await.unwrap();

    assert!(response.starts_with("HTTP/1.1 200 OK"), "{response}");
    assert!(
        response
            .to_ascii_lowercase()
            .contains("strict-transport-security:")
    );
    assert!(
        response
            .to_ascii_lowercase()
            .contains("content-security-policy:")
    );
    assert!(response.ends_with("two"), "{response}");
}

#[tokio::test]
async fn http_server_applies_cors_body_limit_and_request_timeout() {
    async fn echo(body: String) -> String {
        body
    }

    async fn slow() -> &'static str {
        tokio::time::sleep(Duration::from_millis(200)).await;
        "late"
    }

    let config = HttpServerConfig {
        request_timeout: Duration::from_millis(25),
        max_body_bytes: 4,
        cors: Some(CorsPolicy {
            allowed_origins: vec!["https://example.com".to_string()],
            allowed_methods: vec!["POST".to_string()],
            allowed_headers: vec!["authorization".to_string()],
            max_age: Duration::from_secs(60),
            ..CorsPolicy::default()
        }),
        ..loopback_http_config(0)
    };
    let server = HttpServerBuilder::new(config, CancellationToken::new())
        .with_router(
            Router::new()
                .route("/echo", post(echo))
                .route("/slow", get(slow)),
        )
        .build()
        .unwrap();

    server.start().await.unwrap();
    let port = assigned_port(&server);
    let cors = raw_http_request(
        port,
        concat!(
            "OPTIONS /echo HTTP/1.1\r\n",
            "Host: localhost\r\n",
            "Origin: https://example.com\r\n",
            "Access-Control-Request-Method: POST\r\n",
            "Access-Control-Request-Headers: authorization\r\n",
            "Connection: close\r\n\r\n"
        ),
    )
    .await;
    let too_large = raw_http_request(
        port,
        "POST /echo HTTP/1.1\r\nHost: localhost\r\nContent-Length: 5\r\nConnection: close\r\n\r\n12345",
    )
    .await;
    let timed_out = raw_http_request(
        port,
        "GET /slow HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n",
    )
    .await;
    server.stop().await.unwrap();

    assert!(cors.starts_with("HTTP/1.1 200 OK"), "{cors}");
    assert!(
        cors.to_ascii_lowercase()
            .contains("access-control-allow-origin: https://example.com")
    );
    assert!(
        too_large.starts_with("HTTP/1.1 413 Payload Too Large"),
        "{too_large}"
    );
    assert!(
        timed_out.starts_with("HTTP/1.1 408 Request Timeout"),
        "{timed_out}"
    );
}

#[tokio::test]
async fn http_server_rejects_second_start_after_router_is_consumed() {
    let server = HttpServerBuilder::new(loopback_http_config(0), CancellationToken::new())
        .with_router(Router::new().route("/", get(|| async { "ok" })))
        .build()
        .unwrap();

    server.start().await.unwrap();
    let second_start = server.start().await;
    server.stop().await.unwrap();

    assert!(second_start.is_err());
    assert!(
        second_start
            .unwrap_err()
            .to_string()
            .contains("HTTP server already started")
    );
}

#[tokio::test]
async fn http_server_rejects_invalid_bind_address_and_missing_tls_files() {
    let invalid_addr = HttpServerBuilder::new(
        HttpServerConfig {
            host: "not a socket address".to_string(),
            port: 8080,
            ..HttpServerConfig::default()
        },
        CancellationToken::new(),
    )
    .build()
    .unwrap();

    let invalid_addr_error = invalid_addr.start().await.unwrap_err();
    assert!(
        invalid_addr_error
            .to_string()
            .contains("invalid bind address")
    );

    let missing_tls = HttpServerBuilder::new(
        HttpServerConfig {
            tls: Some(HttpTlsConfig {
                cert_file: Some("core/rskit-server/tests/fixtures/missing-cert.pem".to_string()),
                key_file: Some("core/rskit-server/tests/fixtures/missing-key.pem".to_string()),
                min_version: TlsVersion::Tls13,
                ..HttpTlsConfig::default()
            }),
            ..loopback_http_config(0)
        },
        CancellationToken::new(),
    )
    .build()
    .unwrap();

    let tls_error = missing_tls.start().await.unwrap_err();
    assert!(
        tls_error
            .to_string()
            .contains("failed to load HTTP TLS certificate file")
    );
}
