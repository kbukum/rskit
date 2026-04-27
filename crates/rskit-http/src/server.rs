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

use crate::config::{CorsConfig, HttpServerConfig};

/// Builder for [`HttpServer`].
pub struct HttpServerBuilder {
    config: HttpServerConfig,
    cancel: CancellationToken,
    router: Router,
}

impl HttpServerBuilder {
    /// Create a new builder.
    pub fn new(config: HttpServerConfig, cancel: CancellationToken) -> Self {
        Self {
            config,
            cancel,
            router: Router::new(),
        }
    }

    /// Merge an axum [`Router`] into the server.
    #[must_use]
    pub fn with_router(mut self, router: Router) -> Self {
        self.router = self.router.merge(router);
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

    /// Add automatic tracing span per request.
    ///
    /// SECURITY(#29): only the request path is recorded in spans — query strings
    /// are intentionally omitted because they may contain bearer tokens, API keys,
    /// or other secrets that must not appear in distributed trace backends.
    #[must_use]
    pub fn with_tracing(mut self) -> Self {
        use http::Request;
        use tower_http::trace::DefaultOnResponse;
        use tracing::Level;

        let trace_layer = TraceLayer::new_for_http()
            .make_span_with(|request: &Request<_>| {
                // Record path only — query strings may contain sensitive tokens.
                let path = request.uri().path();
                tracing::info_span!(
                    "http_request",
                    method = %request.method(),
                    // http.target intentionally excludes the query string (see issue #29)
                    "http.target" = path,
                    status_code = tracing::field::Empty,
                )
            })
            .on_response(DefaultOnResponse::new().level(Level::INFO));

        self.router = self.router.layer(trace_layer);
        self
    }

    /// Consume the builder and produce an [`HttpServer`].
    pub fn build(self) -> HttpServer {
        HttpServer {
            config: Arc::new(self.config),
            cancel: self.cancel,
            router: Arc::new(tokio::sync::Mutex::new(Some(self.router))),
        }
    }
}

fn build_cors_layer(cfg: &CorsConfig) -> CorsLayer {
    use tower_http::cors::AllowOrigin;
    let origins: Vec<_> = cfg
        .allowed_origins
        .iter()
        .filter_map(|o| o.parse().ok())
        .collect();
    // WARNING: Wildcard CORS is only safe in development.
    // Production deployments must use an explicit origin allowlist.
    CorsLayer::new().allow_origin(AllowOrigin::list(origins))
}

/// HTTP server that implements the [`Component`] lifecycle.
pub struct HttpServer {
    config: Arc<HttpServerConfig>,
    cancel: CancellationToken,
    // TODO(#4): Replace Mutex<Option<Router>> with HttpServer<Stopped|Bound|Running>
    // typestate to make double-start a compile-time error rather than a runtime one.
    // See issue #4 for the full typestate design.
    router: Arc<tokio::sync::Mutex<Option<Router>>>,
}

impl HttpServer {
    /// Bind address as `"host:port"`.
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

        let addr: std::net::SocketAddr = self.config.bind_addr().parse().map_err(|e| {
            AppError::new(ErrorCode::Internal, format!("invalid bind address: {e}"))
        })?;

        let cancel = self.cancel.clone();
        tokio::spawn(async move {
            let listener = match tokio::net::TcpListener::bind(addr).await {
                Ok(l) => l,
                Err(e) => {
                    tracing::error!(error = ?e, %addr, "HTTP server bind failed");
                    return;
                }
            };
            tracing::info!(%addr, "HTTP server listening");
            if let Err(e) = axum::serve(listener, router)
                .with_graceful_shutdown(async move { cancel.cancelled().await })
                .await
            {
                tracing::error!(error = ?e, "HTTP server error");
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
            let registry = registry.clone();
            move || {
                let registry = registry.clone();
                async move {
                    let healths = registry.health_all();
                    let all_ok = healths.iter().all(|h| h.is_healthy());
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
///
/// Unlike [`health_router`], this endpoint requires no registry and always
/// returns `200 OK` as long as the process is running — suitable for
/// Kubernetes liveness probes.
///
/// # Examples
///
/// ```rust,ignore
/// let app = HttpServerBuilder::new(config, cancel)
///     .with_router(healthz_router())
///     .build();
/// ```
pub fn healthz_router() -> Router {
    use axum::{Json, routing::get};
    use serde::Serialize;

    #[derive(Serialize)]
    struct LivenessResponse {
        status: &'static str,
        version: &'static str,
    }

    #[tracing::instrument]
    async fn healthz_handler() -> Json<LivenessResponse> {
        Json(LivenessResponse {
            status: "ok",
            version: env!("CARGO_PKG_VERSION"),
        })
    }

    Router::new().route("/healthz", get(healthz_handler))
}
