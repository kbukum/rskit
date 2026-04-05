use std::sync::Arc;
use std::time::Duration;

use axum::http::{Request, StatusCode};
use axum::response::IntoResponse;
use axum::{Json, Router, routing::get};
use rskit_bootstrap::{Component, Health, Registry};
use rskit_errors::{AppError, ErrorCode};
use rskit_http::{
    CorsConfig, CorrelationId, ErrorHandlerLayer, HttpError, HttpServerBuilder, HttpServerConfig,
    RequestId, health_router,
};
use tokio_util::sync::CancellationToken;
use tower::ServiceExt;
use validator::Validate;

// ═══════════════════════════════════════════════════════════════════════════════
// Config tests
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn config_defaults_all_fields() {
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
fn config_bind_addr_default() {
    let cfg = HttpServerConfig::default();
    assert_eq!(cfg.bind_addr(), "0.0.0.0:8080");
}

#[test]
fn config_bind_addr_custom_host_port() {
    let cfg = HttpServerConfig {
        host: "127.0.0.1".into(),
        port: 3000,
        ..Default::default()
    };
    assert_eq!(cfg.bind_addr(), "127.0.0.1:3000");
}

#[test]
fn config_bind_addr_ipv6() {
    let cfg = HttpServerConfig {
        host: "::1".into(),
        port: 9090,
        ..Default::default()
    };
    assert_eq!(cfg.bind_addr(), "::1:9090");
}

#[test]
fn config_bind_addr_min_port() {
    let cfg = HttpServerConfig {
        port: 1,
        ..Default::default()
    };
    assert_eq!(cfg.bind_addr(), "0.0.0.0:1");
}

#[test]
fn config_bind_addr_max_port() {
    let cfg = HttpServerConfig {
        port: 65535,
        ..Default::default()
    };
    assert_eq!(cfg.bind_addr(), "0.0.0.0:65535");
}

#[test]
fn config_validation_succeeds_for_defaults() {
    let cfg = HttpServerConfig::default();
    assert!(cfg.validate().is_ok());
}

#[test]
fn config_custom_timeouts() {
    let cfg = HttpServerConfig {
        read_timeout: Duration::from_secs(5),
        write_timeout: Duration::from_secs(10),
        idle_timeout: Duration::from_secs(120),
        ..Default::default()
    };
    assert_eq!(cfg.read_timeout, Duration::from_secs(5));
    assert_eq!(cfg.write_timeout, Duration::from_secs(10));
    assert_eq!(cfg.idle_timeout, Duration::from_secs(120));
}

#[test]
fn config_h2c_can_be_disabled() {
    let cfg = HttpServerConfig {
        enable_h2c: false,
        ..Default::default()
    };
    assert!(!cfg.enable_h2c);
}

#[test]
fn config_with_cors() {
    let cfg = HttpServerConfig {
        cors: Some(CorsConfig {
            allowed_origins: vec!["http://localhost:3000".into(), "https://example.com".into()],
            allowed_methods: vec!["GET".into(), "POST".into(), "PUT".into()],
            allowed_headers: vec!["Content-Type".into(), "Authorization".into()],
            allow_credentials: true,
            max_age: Duration::from_secs(3600),
        }),
        ..Default::default()
    };
    let cors = cfg.cors.as_ref().unwrap();
    assert_eq!(cors.allowed_origins.len(), 2);
    assert_eq!(cors.allowed_methods.len(), 3);
    assert_eq!(cors.allowed_headers.len(), 2);
    assert!(cors.allow_credentials);
    assert_eq!(cors.max_age, Duration::from_secs(3600));
}

#[test]
fn config_deserialize_from_json_with_defaults() {
    let json = r#"{"port": 9090}"#;
    let cfg: HttpServerConfig = serde_json::from_str(json).unwrap();
    assert_eq!(cfg.port, 9090);
    assert_eq!(cfg.host, "0.0.0.0");
    assert_eq!(cfg.read_timeout, Duration::from_secs(30));
    assert!(cfg.cors.is_none());
}

#[test]
fn config_deserialize_empty_json() {
    let json = "{}";
    let cfg: HttpServerConfig = serde_json::from_str(json).unwrap();
    assert_eq!(cfg.host, "0.0.0.0");
    assert_eq!(cfg.port, 8080);
}

// ═══════════════════════════════════════════════════════════════════════════════
// CORS config tests
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn cors_config_empty_origins() {
    let cors = CorsConfig {
        allowed_origins: vec![],
        allowed_methods: vec!["GET".into()],
        allowed_headers: vec![],
        allow_credentials: false,
        max_age: Duration::from_secs(0),
    };
    assert!(cors.allowed_origins.is_empty());
}

#[test]
fn cors_config_multiple_origins() {
    let cors = CorsConfig {
        allowed_origins: vec![
            "http://a.com".into(),
            "http://b.com".into(),
            "http://c.com".into(),
        ],
        allowed_methods: vec!["GET".into()],
        allowed_headers: vec![],
        allow_credentials: false,
        max_age: Duration::from_secs(86400),
    };
    assert_eq!(cors.allowed_origins.len(), 3);
    assert_eq!(cors.max_age, Duration::from_secs(86400));
}

// ═══════════════════════════════════════════════════════════════════════════════
// HttpError → IntoResponse tests
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn http_error_not_found_into_response() {
    let app_err = AppError::not_found("user", Some("42"));
    let http_err = HttpError::from(app_err);
    let response = http_err.into_response();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert_eq!(
        response.headers().get("content-type").unwrap(),
        "application/problem+json"
    );
}

#[test]
fn http_error_internal_into_response() {
    let app_err = AppError::new(ErrorCode::Internal, "something broke");
    let http_err = HttpError::from(app_err);
    let response = http_err.into_response();

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

#[test]
fn http_error_unauthorized_into_response() {
    let app_err = AppError::new(ErrorCode::Unauthorized, "bad token");
    let http_err = HttpError::from(app_err);
    let response = http_err.into_response();

    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    assert_eq!(
        response.headers().get("content-type").unwrap(),
        "application/problem+json"
    );
}

#[test]
fn http_error_forbidden_into_response() {
    let app_err = AppError::new(ErrorCode::Forbidden, "access denied");
    let http_err = HttpError::from(app_err);
    let response = http_err.into_response();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
}

#[test]
fn http_error_conflict_into_response() {
    let app_err = AppError::new(ErrorCode::Conflict, "duplicate entry");
    let http_err = HttpError::from(app_err);
    let response = http_err.into_response();

    assert_eq!(response.status(), StatusCode::CONFLICT);
}

#[test]
fn http_error_bad_request_into_response() {
    let app_err = AppError::new(ErrorCode::InvalidInput, "bad field");
    let http_err = HttpError::from(app_err);
    let response = http_err.into_response();

    // InvalidInput maps to 422 UNPROCESSABLE_ENTITY in rskit-errors
    assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
}

#[test]
fn http_error_rate_limited_into_response() {
    let app_err = AppError::new(ErrorCode::RateLimited, "too many requests");
    let http_err = HttpError::from(app_err);
    let response = http_err.into_response();

    assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
}

#[test]
fn http_error_service_unavailable_into_response() {
    let app_err = AppError::new(ErrorCode::ServiceUnavailable, "down");
    let http_err = HttpError::from(app_err);
    let response = http_err.into_response();

    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
}

#[tokio::test]
async fn http_error_response_body_is_json() {
    let app_err = AppError::new(ErrorCode::NotFound, "item not found");
    let http_err = HttpError::from(app_err);
    let response = http_err.into_response();

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();

    // RFC 7807 ErrorResponse fields: type, title, status, detail
    assert!(json.get("type").is_some(), "expected 'type' field in error response");
    assert!(json.get("title").is_some(), "expected 'title' field in error response");
    assert!(json.get("status").is_some(), "expected 'status' field in error response");
    assert!(json.get("detail").is_some(), "expected 'detail' field in error response");
    assert_eq!(json["detail"], "item not found");
    assert_eq!(json["status"], 404);
}

#[test]
fn http_error_from_conversion() {
    let app_err = AppError::new(ErrorCode::Internal, "test");
    let http_err: HttpError = app_err.into();
    assert_eq!(http_err.0.code, ErrorCode::Internal);
}

// ═══════════════════════════════════════════════════════════════════════════════
// Extractor tests (using axum test helpers)
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn request_id_from_header() {
    let app = Router::new().route(
        "/test",
        get(|id: RequestId| async move { id.0 }),
    );

    let response = app
        .oneshot(
            Request::builder()
                .uri("/test")
                .header("x-request-id", "custom-req-id")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(body, "custom-req-id");
}

#[tokio::test]
async fn request_id_generates_uuid_when_missing() {
    let app = Router::new().route(
        "/test",
        get(|id: RequestId| async move { id.0 }),
    );

    let response = app
        .oneshot(
            Request::builder()
                .uri("/test")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let id = String::from_utf8(body.to_vec()).unwrap();
    // UUID v4 format: 8-4-4-4-12
    assert_eq!(id.len(), 36);
    assert_eq!(id.chars().filter(|c| *c == '-').count(), 4);
}

#[tokio::test]
async fn correlation_id_from_header() {
    let app = Router::new().route(
        "/test",
        get(|id: CorrelationId| async move { id.0 }),
    );

    let response = app
        .oneshot(
            Request::builder()
                .uri("/test")
                .header("x-correlation-id", "corr-abc-123")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(body, "corr-abc-123");
}

#[tokio::test]
async fn correlation_id_generates_uuid_when_missing() {
    let app = Router::new().route(
        "/test",
        get(|id: CorrelationId| async move { id.0 }),
    );

    let response = app
        .oneshot(
            Request::builder()
                .uri("/test")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let id = String::from_utf8(body.to_vec()).unwrap();
    assert_eq!(id.len(), 36);
}

#[tokio::test]
async fn request_id_and_correlation_id_together() {
    let app = Router::new().route(
        "/test",
        get(|req_id: RequestId, corr_id: CorrelationId| async move {
            format!("{}|{}", req_id.0, corr_id.0)
        }),
    );

    let response = app
        .oneshot(
            Request::builder()
                .uri("/test")
                .header("x-request-id", "req-1")
                .header("x-correlation-id", "corr-1")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    assert_eq!(body, "req-1|corr-1");
}

// ═══════════════════════════════════════════════════════════════════════════════
// Error handler layer tests
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn error_handler_layer_passes_through_ok_response() {
    let app = Router::new()
        .route("/ok", get(|| async { "hello" }))
        .layer(ErrorHandlerLayer);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/ok")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn error_handler_layer_passes_through_error_response() {
    let app = Router::new()
        .route(
            "/err",
            get(|| async { StatusCode::INTERNAL_SERVER_ERROR }),
        )
        .layer(ErrorHandlerLayer);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/err")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
}

// ═══════════════════════════════════════════════════════════════════════════════
// Health router tests
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn health_router_empty_registry_returns_ok() {
    let registry = Arc::new(Registry::new());
    let app = health_router(registry);

    let response = app
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(json.is_array());
    assert_eq!(json.as_array().unwrap().len(), 0);
}

#[tokio::test]
async fn health_router_healthy_components_return_ok() {
    use async_trait::async_trait;
    use rskit_errors::AppResult;

    struct HealthyComponent;

    #[async_trait]
    impl Component for HealthyComponent {
        fn name(&self) -> &str {
            "test-svc"
        }
        async fn start(&self) -> AppResult<()> {
            Ok(())
        }
        async fn stop(&self) -> AppResult<()> {
            Ok(())
        }
        fn health(&self) -> Health {
            Health::healthy("test-svc")
        }
    }

    let mut registry = Registry::new();
    registry.register(Arc::new(HealthyComponent));
    let app = health_router(Arc::new(registry));

    let response = app
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let arr = json.as_array().unwrap();
    assert_eq!(arr.len(), 1);
    assert_eq!(arr[0]["status"], "healthy");
}

#[tokio::test]
async fn health_router_unhealthy_component_returns_503() {
    use async_trait::async_trait;
    use rskit_errors::AppResult;

    struct UnhealthyComponent;

    #[async_trait]
    impl Component for UnhealthyComponent {
        fn name(&self) -> &str {
            "broken-svc"
        }
        async fn start(&self) -> AppResult<()> {
            Ok(())
        }
        async fn stop(&self) -> AppResult<()> {
            Ok(())
        }
        fn health(&self) -> Health {
            Health::unhealthy("broken-svc", "database down")
        }
    }

    let mut registry = Registry::new();
    registry.register(Arc::new(UnhealthyComponent));
    let app = health_router(Arc::new(registry));

    let response = app
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    let arr = json.as_array().unwrap();
    assert_eq!(arr[0]["status"], "unhealthy");
    assert_eq!(arr[0]["message"], "database down");
}

#[tokio::test]
async fn health_router_mixed_healthy_unhealthy_returns_503() {
    use async_trait::async_trait;
    use rskit_errors::AppResult;

    struct MockComp {
        name: &'static str,
        healthy: bool,
    }

    #[async_trait]
    impl Component for MockComp {
        fn name(&self) -> &str {
            self.name
        }
        async fn start(&self) -> AppResult<()> {
            Ok(())
        }
        async fn stop(&self) -> AppResult<()> {
            Ok(())
        }
        fn health(&self) -> Health {
            if self.healthy {
                Health::healthy(self.name)
            } else {
                Health::unhealthy(self.name, "down")
            }
        }
    }

    let mut registry = Registry::new();
    registry.register(Arc::new(MockComp {
        name: "ok-svc",
        healthy: true,
    }));
    registry.register(Arc::new(MockComp {
        name: "bad-svc",
        healthy: false,
    }));
    let app = health_router(Arc::new(registry));

    let response = app
        .oneshot(
            Request::builder()
                .uri("/health")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    // If any component is unhealthy, return 503
    assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
}

// ═══════════════════════════════════════════════════════════════════════════════
// Server builder tests
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn builder_with_router_merges() {
    let cancel = CancellationToken::new();
    let cfg = HttpServerConfig::default();

    let custom_router = Router::new().route("/custom", get(|| async { "custom" }));
    let server = HttpServerBuilder::new(cfg, cancel)
        .with_router(custom_router)
        .build();

    assert_eq!(server.bind_addr(), "0.0.0.0:8080");
}

#[tokio::test]
async fn builder_with_cors_no_config_is_noop() {
    let cancel = CancellationToken::new();
    let cfg = HttpServerConfig {
        cors: None,
        ..Default::default()
    };

    // Should not panic
    let _server = HttpServerBuilder::new(cfg, cancel).with_cors().build();
}

#[tokio::test]
async fn builder_with_cors_config_applies_layer() {
    let cancel = CancellationToken::new();
    let cfg = HttpServerConfig {
        cors: Some(CorsConfig {
            allowed_origins: vec!["http://localhost:3000".into()],
            allowed_methods: vec!["GET".into()],
            allowed_headers: vec![],
            allow_credentials: false,
            max_age: Duration::from_secs(300),
        }),
        ..Default::default()
    };

    // Should not panic
    let _server = HttpServerBuilder::new(cfg, cancel).with_cors().build();
}

#[tokio::test]
async fn builder_with_request_id() {
    let cancel = CancellationToken::new();
    let cfg = HttpServerConfig::default();

    // Should not panic
    let _server = HttpServerBuilder::new(cfg, cancel).with_request_id().build();
}

#[tokio::test]
async fn builder_with_tracing() {
    let cancel = CancellationToken::new();
    let cfg = HttpServerConfig::default();

    // Should not panic
    let _server = HttpServerBuilder::new(cfg, cancel).with_tracing().build();
}

#[tokio::test]
async fn builder_full_chain() {
    let cancel = CancellationToken::new();
    let cfg = HttpServerConfig {
        cors: Some(CorsConfig {
            allowed_origins: vec!["http://localhost".into()],
            allowed_methods: vec!["GET".into()],
            allowed_headers: vec!["Content-Type".into()],
            allow_credentials: false,
            max_age: Duration::from_secs(60),
        }),
        ..Default::default()
    };

    let router = Router::new().route("/api", get(|| async { "api" }));

    // Full builder chain should not panic
    let _server = HttpServerBuilder::new(cfg, cancel)
        .with_router(router)
        .with_cors()
        .with_request_id()
        .with_tracing()
        .build();
}

// ═══════════════════════════════════════════════════════════════════════════════
// Server lifecycle tests
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn server_start_and_stop() {
    let cancel = CancellationToken::new();
    let cfg = HttpServerConfig {
        host: "127.0.0.1".into(),
        port: 0, // OS picks free port
        ..Default::default()
    };

    let server = HttpServerBuilder::new(cfg, cancel.clone())
        .with_request_id()
        .build();

    // Start should succeed
    server.start().await.unwrap();

    // Health should report healthy
    let health = server.health();
    assert!(health.is_healthy());
    assert_eq!(health.name, "http-server");

    // Name
    assert_eq!(server.name(), "http-server");

    // Stop
    server.stop().await.unwrap();
}

#[tokio::test]
async fn server_double_start_returns_error() {
    let cancel = CancellationToken::new();
    let cfg = HttpServerConfig {
        host: "127.0.0.1".into(),
        port: 0,
        ..Default::default()
    };

    let server = HttpServerBuilder::new(cfg, cancel.clone()).build();

    // First start succeeds
    server.start().await.unwrap();

    // Second start should error (router already taken)
    let result = server.start().await;
    assert!(result.is_err());

    cancel.cancel();
    server.stop().await.unwrap();
}

// ═══════════════════════════════════════════════════════════════════════════════
// Route handling integration tests
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn route_returns_json_response() {
    #[derive(serde::Serialize)]
    struct Item {
        id: u32,
        name: String,
    }

    let app = Router::new().route(
        "/item",
        get(|| async {
            Json(Item {
                id: 1,
                name: "test".into(),
            })
        }),
    );

    let response = app
        .oneshot(
            Request::builder()
                .uri("/item")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = axum::body::to_bytes(response.into_body(), usize::MAX)
        .await
        .unwrap();
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["id"], 1);
    assert_eq!(json["name"], "test");
}

#[tokio::test]
async fn route_404_for_missing_path() {
    let app = Router::new().route("/exists", get(|| async { "ok" }));

    let response = app
        .oneshot(
            Request::builder()
                .uri("/not-exists")
                .body(axum::body::Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

// ═══════════════════════════════════════════════════════════════════════════════
// Concurrent request handling
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn concurrent_requests_to_router() {
    use std::sync::atomic::{AtomicU32, Ordering};

    let counter = Arc::new(AtomicU32::new(0));
    let counter_clone = counter.clone();

    let app = Router::new().route(
        "/count",
        get(move || {
            let c = counter_clone.clone();
            async move {
                c.fetch_add(1, Ordering::SeqCst);
                "ok"
            }
        }),
    );

    // Send multiple requests in parallel
    let mut handles = Vec::new();
    for _ in 0..10 {
        let app = app.clone();
        handles.push(tokio::spawn(async move {
            let resp = app
                .oneshot(
                    Request::builder()
                        .uri("/count")
                        .body(axum::body::Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            resp.status()
        }));
    }

    for handle in handles {
        let status = handle.await.unwrap();
        assert_eq!(status, StatusCode::OK);
    }

    assert_eq!(counter.load(Ordering::SeqCst), 10);
}

// ═══════════════════════════════════════════════════════════════════════════════
// Error mapping (AppError → HTTP response) comprehensive
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn error_code_to_status_mapping() {
    let test_cases: Vec<(ErrorCode, u16)> = vec![
        (ErrorCode::NotFound, 404),
        (ErrorCode::Unauthorized, 401),
        (ErrorCode::Forbidden, 403),
        (ErrorCode::InvalidInput, 422),  // maps to UNPROCESSABLE_ENTITY
        (ErrorCode::Conflict, 409),
        (ErrorCode::RateLimited, 429),
        (ErrorCode::Internal, 500),
        (ErrorCode::ServiceUnavailable, 503),
        (ErrorCode::Timeout, 504),
    ];

    for (code, expected_status) in test_cases {
        let err = AppError::new(code, "test");
        let http_err = HttpError::from(err);
        let resp = http_err.into_response();
        assert_eq!(
            resp.status().as_u16(),
            expected_status,
            "ErrorCode::{:?} should map to HTTP {}",
            code,
            expected_status,
        );
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// Extractor struct tests
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn request_id_debug_and_clone() {
    let id = RequestId("test-id".into());
    let cloned = id.clone();
    assert_eq!(cloned.0, "test-id");
    let debug = format!("{:?}", id);
    assert!(debug.contains("test-id"));
}

#[test]
fn correlation_id_debug_and_clone() {
    let id = CorrelationId("corr-id".into());
    let cloned = id.clone();
    assert_eq!(cloned.0, "corr-id");
    let debug = format!("{:?}", id);
    assert!(debug.contains("corr-id"));
}

// ═══════════════════════════════════════════════════════════════════════════════
// Config Clone and Debug
// ═══════════════════════════════════════════════════════════════════════════════

#[test]
fn config_clone_produces_equal() {
    let cfg = HttpServerConfig {
        host: "10.0.0.1".into(),
        port: 4000,
        ..Default::default()
    };
    let cloned = cfg.clone();
    assert_eq!(cloned.host, "10.0.0.1");
    assert_eq!(cloned.port, 4000);
}

#[test]
fn config_debug_output() {
    let cfg = HttpServerConfig::default();
    let debug = format!("{:?}", cfg);
    assert!(debug.contains("0.0.0.0"));
    assert!(debug.contains("8080"));
}

#[test]
fn cors_config_clone_and_debug() {
    let cors = CorsConfig {
        allowed_origins: vec!["http://test.com".into()],
        allowed_methods: vec!["GET".into()],
        allowed_headers: vec![],
        allow_credentials: true,
        max_age: Duration::from_secs(60),
    };
    let cloned = cors.clone();
    assert_eq!(cloned.allowed_origins, cors.allowed_origins);
    let debug = format!("{:?}", cors);
    assert!(debug.contains("test.com"));
}
