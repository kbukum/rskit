use std::sync::Arc;
use std::time::Duration;

use axum::extract::DefaultBodyLimit;
use axum::{Router, http::StatusCode};
use parking_lot::Mutex;
use rskit_errors::AppResult;
use rskit_http::{SecurityHeadersConfig, SecurityHeadersLayer};
use tokio_util::sync::CancellationToken;
use tower_http::{
    request_id::{MakeRequestUuid, SetRequestIdLayer},
    timeout::TimeoutLayer,
    trace::TraceLayer,
};

use super::component::HttpServer;
use crate::http_config::HttpServerConfig;
use crate::middleware::HttpMiddlewareStack;

/// Builder for [`HttpServer`].
pub struct HttpServerBuilder {
    config: HttpServerConfig,
    cancel: CancellationToken,
    router: Router,
    middleware: HttpMiddlewareStack,
    security_headers: Option<SecurityHeadersConfig>,
}

impl HttpServerBuilder {
    /// Create a new builder.
    pub fn new(config: HttpServerConfig, cancel: CancellationToken) -> Self {
        Self {
            config,
            cancel,
            router: Router::new(),
            middleware: HttpMiddlewareStack::new(),
            security_headers: None,
        }
    }

    /// Merge an axum [`Router`] into the server.
    #[must_use]
    pub fn with_router(mut self, router: Router) -> Self {
        self.router = self.router.merge(router);
        self
    }

    /// Replace the ordered middleware stack.
    #[must_use]
    pub fn with_middleware_stack(mut self, middleware: HttpMiddlewareStack) -> Self {
        self.middleware = middleware;
        self
    }

    /// Append a logging-phase transform.
    #[must_use]
    pub fn with_logging_transform<F>(mut self, transform: F) -> Self
    where
        F: Fn(Router) -> Router + Send + Sync + 'static,
    {
        self.middleware = self.middleware.with_logging_transform(transform);
        self
    }

    /// Append an auth-phase transform.
    #[must_use]
    pub fn with_auth_transform<F>(mut self, transform: F) -> Self
    where
        F: Fn(Router) -> Router + Send + Sync + 'static,
    {
        self.middleware = self.middleware.with_auth_transform(transform);
        self
    }

    /// Append a validation-phase transform.
    #[must_use]
    pub fn with_validation_transform<F>(mut self, transform: F) -> Self
    where
        F: Fn(Router) -> Router + Send + Sync + 'static,
    {
        self.middleware = self.middleware.with_validation_transform(transform);
        self
    }

    /// Append a metrics-phase transform.
    #[must_use]
    pub fn with_metrics_transform<F>(mut self, transform: F) -> Self
    where
        F: Fn(Router) -> Router + Send + Sync + 'static,
    {
        self.middleware = self.middleware.with_metrics_transform(transform);
        self
    }

    /// Apply CORS from the server config (no-op if `cors` is `None`).
    ///
    /// # Errors
    /// Returns an error when the configured CORS policy contains invalid origins, methods, headers,
    /// or max-age values.
    #[must_use = "builder methods return a new builder; use the returned value"]
    pub fn with_cors(self) -> AppResult<Self> {
        if let Some(cors_cfg) = self.config.cors.as_ref() {
            let _ = cors_cfg.layer()?;
        }
        Ok(self)
    }

    /// Add secure response headers using the default security policy.
    ///
    /// # Errors
    /// Returns an error if the default security policy cannot be built (should never happen in practice — this is a programming error guard).
    #[must_use = "builder methods return a new builder; use the returned value"]
    pub fn with_security_headers(self) -> AppResult<Self> {
        self.with_security_headers_config(SecurityHeadersConfig::default())
    }

    /// Add secure response headers using an explicit security policy.
    ///
    /// # Errors
    /// Returns an error when the supplied policy is invalid.
    #[must_use = "builder methods return a new builder; use the returned value"]
    pub fn with_security_headers_config(
        mut self,
        config: SecurityHeadersConfig,
    ) -> AppResult<Self> {
        let _ = SecurityHeadersLayer::new(&config)?;
        self.security_headers = Some(config);
        Ok(self)
    }

    /// Consume the builder and produce an [`HttpServer`].
    /// # Errors
    /// Returns an error when baseline transport middleware configuration is invalid.
    pub fn build(self) -> AppResult<HttpServer> {
        let builder = self;
        let security_headers = builder.security_headers.clone();
        let request_timeout = builder.config.request_timeout;
        let max_body_bytes = builder.config.max_body_bytes;
        let cors = builder.config.cors.clone();
        let router = builder.middleware.apply(builder.router);
        let router = apply_canonical_tracing(router);
        let router = apply_baseline_layers(
            router,
            security_headers,
            request_timeout,
            max_body_bytes,
            cors,
        )?;
        Ok(HttpServer {
            config: Arc::new(builder.config),
            cancel: builder.cancel,
            router: Arc::new(tokio::sync::Mutex::new(Some(router))),
            local_addr: Arc::new(Mutex::new(None)),
        })
    }
}

fn apply_canonical_tracing(router: Router) -> Router {
    use http::Request;
    use tower_http::trace::DefaultOnResponse;
    use tracing::Level;

    let trace_layer = TraceLayer::new_for_http()
        .make_span_with(|request: &Request<_>| {
            let path = request.uri().path();
            tracing::info_span!(
                "http_request",
                method = %request.method(),
                "http.target" = path,
                status_code = tracing::field::Empty,
            )
        })
        .on_response(DefaultOnResponse::new().level(Level::INFO));

    router.layer(trace_layer)
}

fn apply_baseline_layers(
    router: Router,
    security_headers: Option<SecurityHeadersConfig>,
    request_timeout: Duration,
    max_body_bytes: usize,
    cors: Option<crate::CorsPolicy>,
) -> AppResult<Router> {
    let security_headers = SecurityHeadersLayer::new(&security_headers.unwrap_or_default())?;
    let router = router
        .layer(TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            request_timeout,
        ))
        .layer(DefaultBodyLimit::max(max_body_bytes))
        .layer(security_headers);
    let router = if let Some(cors_cfg) = cors.as_ref() {
        router.layer(cors_cfg.layer()?)
    } else {
        router
    };
    Ok(router.layer(SetRequestIdLayer::x_request_id(MakeRequestUuid)))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use axum::body::Body;
    use axum::{http::Request, routing::get};
    use rskit_bootstrap::Component;
    use rskit_errors::ErrorCode;
    use rskit_http::CorsPolicy;
    use rskit_security::TransportSecurity;
    use tower::ServiceExt;

    use super::*;
    use crate::http::test_support::local_config;

    #[tokio::test]
    async fn builder_applies_baseline_layers_to_application_routes() {
        let server = HttpServerBuilder::new(local_config(), CancellationToken::new())
            .with_router(Router::new().route("/", get(|| async { "ok" })))
            .with_security_headers_config(
                SecurityHeadersConfig::default()
                    .with_transport_security(TransportSecurity::AllowInsecureLocal),
            )
            .expect("configure security headers")
            .build()
            .expect("build server");
        let router = server
            .router
            .lock()
            .await
            .take()
            .expect("router is present");

        let response = router
            .oneshot(
                Request::builder()
                    .uri("/")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("route response");

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response
                .headers()
                .get(http::header::X_CONTENT_TYPE_OPTIONS)
                .expect("x-content-type-options header"),
            "nosniff"
        );
    }

    #[tokio::test]
    async fn builder_applies_ordered_transform_helpers_and_cors() {
        let calls = Arc::new(AtomicUsize::new(0));
        let transform = |calls: Arc<AtomicUsize>| {
            move |router: Router| {
                calls.fetch_add(1, Ordering::SeqCst);
                router
            }
        };

        let server = HttpServerBuilder::new(local_config(), CancellationToken::new())
            .with_router(Router::new().route("/", get(|| async { "ok" })))
            .with_logging_transform(transform(Arc::clone(&calls)))
            .with_auth_transform(transform(Arc::clone(&calls)))
            .with_validation_transform(transform(Arc::clone(&calls)))
            .with_metrics_transform(transform(Arc::clone(&calls)))
            .with_cors()
            .expect("default cors state is valid")
            .build()
            .expect("build server");

        let router = server
            .router
            .lock()
            .await
            .take()
            .expect("router is present");
        let response = router
            .oneshot(
                Request::builder()
                    .uri("/")
                    .body(Body::empty())
                    .expect("request"),
            )
            .await
            .expect("route response");

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(calls.load(Ordering::SeqCst), 4);
    }

    #[test]
    fn builder_helpers_validate_cors_security_headers_and_middleware_stack() {
        let invalid_cors = HttpServerConfig {
            cors: Some(CorsPolicy {
                allow_credentials: true,
                ..Default::default()
            }),
            ..local_config()
        };
        assert_eq!(
            HttpServerBuilder::new(invalid_cors, CancellationToken::new())
                .with_cors()
                .err()
                .unwrap()
                .code(),
            ErrorCode::InvalidInput
        );
        let invalid_cors = HttpServerConfig {
            cors: Some(CorsPolicy {
                allowed_origins: vec!["*".to_string()],
                ..Default::default()
            }),
            ..local_config()
        };
        assert_eq!(
            HttpServerBuilder::new(invalid_cors, CancellationToken::new())
                .build()
                .err()
                .unwrap()
                .code(),
            ErrorCode::InvalidInput
        );

        let builder = HttpServerBuilder::new(local_config(), CancellationToken::new())
            .with_middleware_stack(HttpMiddlewareStack::new())
            .with_security_headers()
            .expect("default security headers are valid");
        assert!(builder.security_headers.is_some());
    }

    #[test]
    fn builder_builds_with_valid_cors_and_exposes_server_accessors() {
        let config = HttpServerConfig {
            cors: Some(CorsPolicy {
                allowed_origins: vec!["https://example.com".to_string()],
                allowed_methods: vec!["GET".to_string()],
                allowed_headers: vec!["x-test".to_string()],
                ..Default::default()
            }),
            ..local_config()
        };
        let server = HttpServerBuilder::new(config, CancellationToken::new())
            .with_router(Router::new())
            .build()
            .expect("valid CORS policy builds");

        assert_eq!(server.name(), "http-server");
        assert_eq!(server.bind_addr(), "127.0.0.1:0");
        assert_eq!(server.local_addr(), None);
    }

    #[tokio::test]
    async fn builder_applies_ordered_transform_shortcuts() {
        let server = HttpServerBuilder::new(local_config(), CancellationToken::new())
            .with_logging_transform(|router| router.route("/logging", get(|| async { "logging" })))
            .with_auth_transform(|router| router.route("/auth", get(|| async { "auth" })))
            .with_validation_transform(|router| {
                router.route("/validation", get(|| async { "validation" }))
            })
            .with_metrics_transform(|router| router.route("/metrics", get(|| async { "metrics" })))
            .with_cors()
            .expect("empty cors config is accepted")
            .build()
            .expect("build server");
        let router = server
            .router
            .lock()
            .await
            .take()
            .expect("router is present");

        for (path, expected) in [
            ("/logging", "logging"),
            ("/auth", "auth"),
            ("/validation", "validation"),
            ("/metrics", "metrics"),
        ] {
            let response = router
                .clone()
                .oneshot(
                    Request::builder()
                        .uri(path)
                        .body(Body::empty())
                        .expect("request"),
                )
                .await
                .expect("route response");
            let body = axum::body::to_bytes(response.into_body(), usize::MAX)
                .await
                .expect("body bytes");
            assert_eq!(&body[..], expected.as_bytes());
        }
    }

    #[test]
    fn security_header_configuration_is_validated_at_build_time() {
        let config = SecurityHeadersConfig::default()
            .with_transport_security(TransportSecurity::AllowInsecureLocal)
            .with_content_security_policy(None)
            .with_permissions_policy(None)
            .with_referrer_policy(None)
            .with_frame_options(None)
            .with_content_type_options(None);

        let error = match HttpServerBuilder::new(local_config(), CancellationToken::new())
            .with_security_headers_config(config)
        {
            Ok(_) => panic!("invalid security header policy should be rejected"),
            Err(error) => error,
        };

        assert_eq!(error.code(), ErrorCode::InvalidInput);
    }
}
