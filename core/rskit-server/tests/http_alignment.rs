use std::sync::Arc;
use std::time::Duration;

use axum::{Router, body::Body, http::Request, routing::get};
use rskit_bootstrap::Registry;
use rskit_server::{
    CorsPolicy, HTTP_INTERCEPTOR_ORDER, HttpMiddlewareStack, HttpServerBuilder, HttpServerConfig,
    health_router, healthz_router,
};
use rskit_validation::Validate;
use tokio_util::sync::CancellationToken;
use tower::ServiceExt;

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
}

#[test]
fn builder_accepts_ordered_middleware_stack() {
    let server = HttpServerBuilder::new(HttpServerConfig::default(), CancellationToken::new())
        .with_middleware_stack(HttpMiddlewareStack::new())
        .with_router(Router::new().route("/", get(|| async { "ok" })))
        .with_request_id()
        .build();
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
