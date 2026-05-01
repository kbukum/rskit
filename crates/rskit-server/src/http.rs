use std::sync::Arc;

use async_trait::async_trait;
use axum::Router;
use rskit_bootstrap::{Component, Health, Registry};
use rskit_errors::{AppError, AppResult, ErrorCode};
use tokio_util::sync::CancellationToken;
use tower_http::{
    cors::CorsLayer,
    request_id::{MakeRequestUuid, SetRequestIdLayer},
    trace::TraceLayer,
};

use crate::http_config::{CorsConfig, HttpServerConfig};
use crate::middleware::HttpMiddlewareStack;

/// Builder for [`HttpServer`].
pub struct HttpServerBuilder {
    config: HttpServerConfig,
    cancel: CancellationToken,
    router: Router,
    middleware: HttpMiddlewareStack,
}

impl HttpServerBuilder {
    /// Create a new builder.
    pub fn new(config: HttpServerConfig, cancel: CancellationToken) -> Self {
        Self {
            config,
            cancel,
            router: Router::new(),
            middleware: HttpMiddlewareStack::new(),
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
    #[must_use]
    pub fn with_cors(mut self) -> Self {
        if let Some(cors_cfg) = &self.config.cors.clone() {
            let cors = build_cors_layer(cors_cfg);
            self.router = self.router.layer(cors);
        }
        self
    }

    /// Add automatic `X-Request-Id` injection.
    #[must_use]
    pub fn with_request_id(mut self) -> Self {
        self.router = self
            .router
            .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid));
        self
    }

    /// Add the canonical tracing phase.
    #[must_use]
    pub fn with_tracing(mut self) -> Self {
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

        self.middleware = self
            .middleware
            .with_tracing_transform(move |router| router.layer(trace_layer.clone()));
        self
    }

    /// Consume the builder and produce an [`HttpServer`].
    pub fn build(self) -> HttpServer {
        HttpServer {
            config: Arc::new(self.config),
            cancel: self.cancel,
            router: Arc::new(tokio::sync::Mutex::new(Some(
                self.middleware.apply(self.router),
            ))),
        }
    }
}

fn build_cors_layer(cfg: &CorsConfig) -> CorsLayer {
    use tower_http::cors::AllowOrigin;
    let origins: Vec<_> = cfg
        .allowed_origins
        .iter()
        .filter_map(|origin| origin.parse().ok())
        .collect();
    CorsLayer::new().allow_origin(AllowOrigin::list(origins))
}

/// HTTP server that implements the [`Component`] lifecycle.
pub struct HttpServer {
    config: Arc<HttpServerConfig>,
    cancel: CancellationToken,
    router: Arc<tokio::sync::Mutex<Option<Router>>>,
}

impl HttpServer {
    /// Bind address as `host:port`.
    #[must_use]
    pub fn bind_addr(&self) -> String {
        self.config.bind_addr()
    }
}

#[async_trait]
impl Component for HttpServer {
    fn name(&self) -> &str {
        "http-server"
    }

    async fn start(&self) -> AppResult<()> {
        let router = self
            .router
            .lock()
            .await
            .take()
            .ok_or_else(|| AppError::new(ErrorCode::Internal, "HTTP server already started"))?;

        let addr: std::net::SocketAddr = self.config.bind_addr().parse().map_err(|error| {
            AppError::new(
                ErrorCode::Internal,
                format!("invalid bind address: {error}"),
            )
        })?;

        let cancel = self.cancel.clone();
        tokio::spawn(async move {
            let listener = match tokio::net::TcpListener::bind(addr).await {
                Ok(listener) => listener,
                Err(error) => {
                    tracing::error!(error = ?error, %addr, "HTTP server bind failed");
                    return;
                }
            };
            tracing::info!(%addr, "HTTP server listening");
            if let Err(error) = axum::serve(listener, router)
                .with_graceful_shutdown(async move { cancel.cancelled().await })
                .await
            {
                tracing::error!(error = ?error, "HTTP server error");
            }
        });

        Ok(())
    }

    async fn stop(&self) -> AppResult<()> {
        self.cancel.cancel();
        Ok(())
    }

    fn health(&self) -> Health {
        Health::healthy("http-server")
    }
}

/// Add a `/health` endpoint returning JSON from a [`Registry`].
pub fn health_router(registry: Arc<Registry>) -> Router {
    use axum::{Json, routing::get};

    Router::new().route(
        "/health",
        get({
            let registry = Arc::clone(&registry);
            move || {
                let registry = Arc::clone(&registry);
                async move {
                    let healths = registry.health_all();
                    let all_ok = healths.iter().all(|health| health.is_healthy());
                    let status = if all_ok {
                        axum::http::StatusCode::OK
                    } else {
                        axum::http::StatusCode::SERVICE_UNAVAILABLE
                    };
                    (status, Json(healths))
                }
            }
        }),
    )
}

/// Returns a router with a `/healthz` liveness probe endpoint.
pub fn healthz_router() -> Router {
    use axum::{Json, routing::get};
    use serde::Serialize;

    #[derive(Serialize)]
    struct LivenessResponse {
        status: &'static str,
        version: &'static str,
    }

    async fn healthz_handler() -> Json<LivenessResponse> {
        Json(LivenessResponse {
            status: "ok",
            version: env!("CARGO_PKG_VERSION"),
        })
    }

    Router::new().route("/healthz", get(healthz_handler))
}
