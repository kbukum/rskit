use std::future::Future;
use std::net::SocketAddr;
use std::pin::pin;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use axum::body::Body;
use axum::extract::DefaultBodyLimit;
use axum::serve::Listener;
use axum::{Router, http::StatusCode};
use hyper::body::Incoming;
use hyper_util::rt::{TokioExecutor, TokioIo, TokioTimer};
use hyper_util::server::conn::auto::Builder as HyperBuilder;
use hyper_util::service::TowerToHyperService;
use parking_lot::Mutex;
use rskit_bootstrap::{Component, Health, Registry};
use rskit_errors::{AppError, AppResult, ErrorCode};
use rskit_http::{SecurityHeadersConfig, SecurityHeadersLayer};
use rskit_security::{TlsConfig, TlsVersion};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, pem::PemObject};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;
use tokio_util::sync::CancellationToken;
use tower::ServiceExt;
use tower_http::{
    request_id::{MakeRequestUuid, SetRequestIdLayer},
    timeout::TimeoutLayer,
    trace::TraceLayer,
};

use crate::http_config::{HttpServerConfig, validate_http_tls_config};
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
    /// Returns an error when the configured CORS policy contains invalid origins,
    /// methods, headers, or max-age values.
    #[must_use = "builder methods return a new builder; use the returned value"]
    pub fn with_cors(self) -> AppResult<Self> {
        if let Some(cors_cfg) = self.config.cors.as_ref() {
            let _ = cors_cfg.layer()?;
        }
        Ok(self)
    }

    /// Add automatic `X-Request-Id` injection.
    #[must_use]
    pub fn with_request_id(self) -> Self {
        self
    }

    /// Add secure response headers using the default security policy.
    ///
    /// # Errors
    /// Returns an error if the default security policy cannot be built (should never happen
    /// in practice — this is a programming error guard).
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

    /// Add the canonical tracing phase.
    ///
    /// Tracing is owned by [`build`](Self::build) so this remains a compatibility no-op.
    #[must_use]
    pub fn with_tracing(self) -> Self {
        self
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

    /// Add the configured request body limit.
    #[must_use]
    pub fn with_body_limit(self) -> Self {
        self
    }

    /// Add the configured request timeout.
    #[must_use]
    pub fn with_timeout(self) -> Self {
        self
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

/// HTTP server that implements the [`Component`] lifecycle.
pub struct HttpServer {
    config: Arc<HttpServerConfig>,
    cancel: CancellationToken,
    router: Arc<tokio::sync::Mutex<Option<Router>>>,
    local_addr: Arc<Mutex<Option<SocketAddr>>>,
}

impl HttpServer {
    /// Bind address as `host:port`.
    #[must_use]
    pub fn bind_addr(&self) -> String {
        self.config.bind_addr()
    }

    /// Actual local socket address after [`start`](Component::start) binds.
    #[must_use]
    pub fn local_addr(&self) -> Option<SocketAddr> {
        *self.local_addr.lock()
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

        let addr: SocketAddr = self.config.bind_addr().parse().map_err(|error| {
            AppError::new(
                ErrorCode::Internal,
                format!("invalid bind address: {error}"),
            )
        })?;
        let tls_acceptor = if let Some(tls) = &self.config.tls {
            Some(build_tls_acceptor(tls)?)
        } else {
            None
        };

        let listener = TcpListener::bind(addr).await.map_err(|error| {
            AppError::new(
                ErrorCode::Internal,
                format!("HTTP server bind failed for {addr}: {error}"),
            )
        })?;
        let actual_addr = listener.local_addr().map_err(|error| {
            AppError::new(
                ErrorCode::Internal,
                format!("failed to inspect HTTP server local address: {error}"),
            )
        })?;
        *self.local_addr.lock() = Some(actual_addr);

        let cancel = self.cancel.clone();
        let config = Arc::clone(&self.config);
        tokio::spawn(async move {
            if let Some(acceptor) = tls_acceptor {
                tracing::info!(addr = %actual_addr, "HTTPS server listening");
                serve_tls_listener(listener, acceptor, router, Arc::clone(&config), cancel).await;
            } else {
                tracing::info!(addr = %actual_addr, "HTTP server listening");
                serve_listener(
                    listener,
                    router,
                    Arc::clone(&config),
                    cancel,
                    config.enable_h2c,
                )
                .await;
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

async fn serve_listener<L>(
    mut listener: L,
    router: Router,
    config: Arc<HttpServerConfig>,
    cancel: CancellationToken,
    allow_h2: bool,
) where
    L: Listener<Addr = SocketAddr> + Send + 'static,
    L::Io: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    loop {
        tokio::select! {
            () = cancel.cancelled() => break,
            (io, addr) = listener.accept() => {
                let router = router.clone();
                let config = Arc::clone(&config);
                let shutdown = cancel.clone().cancelled_owned();
                tokio::spawn(async move {
                    serve_connection(io, addr, router, config, shutdown, allow_h2).await;
                });
            }
        }
    }
}

async fn serve_tls_listener(
    listener: TcpListener,
    acceptor: TlsAcceptor,
    router: Router,
    config: Arc<HttpServerConfig>,
    cancel: CancellationToken,
) {
    loop {
        tokio::select! {
            () = cancel.cancelled() => break,
            result = listener.accept() => {
                match result {
                    Ok((stream, addr)) => {
                        let acceptor = acceptor.clone();
                        let router = router.clone();
                        let config = Arc::clone(&config);
                        let shutdown = cancel.clone().cancelled_owned();
                        tokio::spawn(async move {
                            match tokio::time::timeout(config.read_timeout, acceptor.accept(stream)).await {
                                Ok(Ok(stream)) => {
                                    serve_connection(stream, addr, router, config, shutdown, true).await;
                                }
                                Ok(Err(error)) => {
                                    tracing::warn!(error = ?error, %addr, "TLS handshake failed");
                                }
                                Err(_) => {
                                    tracing::warn!(%addr, "TLS handshake timed out");
                                }
                            }
                        });
                    }
                    Err(error) if is_connection_accept_error(&error) => {}
                    Err(error) => {
                        tracing::error!(error = ?error, "HTTP TLS accept failed");
                        tokio::time::sleep(Duration::from_secs(1)).await;
                    }
                }
            }
        }
    }
}

async fn serve_connection<I, F>(
    io: I,
    addr: SocketAddr,
    router: Router,
    config: Arc<HttpServerConfig>,
    shutdown: F,
    allow_h2: bool,
) where
    I: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    F: Future<Output = ()> + Send + 'static,
{
    let service = router.map_request(|request: http::Request<Incoming>| request.map(Body::new));
    let hyper_service = TowerToHyperService::new(service);
    let mut builder = HyperBuilder::new(TokioExecutor::new());
    builder
        .http1()
        .timer(TokioTimer::new())
        .header_read_timeout(Some(config.read_timeout))
        .keep_alive(true);
    builder
        .http2()
        .timer(TokioTimer::new())
        .keep_alive_interval(Some(config.idle_timeout))
        .keep_alive_timeout(config.read_timeout)
        .enable_connect_protocol();

    let io = TokioIo::new(io);
    if allow_h2 {
        let mut connection = pin!(builder.serve_connection_with_upgrades(io, hyper_service));
        let mut shutdown = pin!(shutdown);

        tokio::select! {
            result = connection.as_mut() => {
                if let Err(error) = result {
                    tracing::debug!(error = ?error, %addr, "HTTP connection finished with error");
                }
            }
            () = &mut shutdown => {
                connection.as_mut().graceful_shutdown();
                if let Err(error) = connection.as_mut().await {
                    tracing::debug!(error = ?error, %addr, "HTTP connection finished with error after graceful shutdown");
                }
            }
        }
    } else {
        let http1 = builder.http1();
        let mut connection = pin!(http1.serve_connection_with_upgrades(io, hyper_service));
        let mut shutdown = pin!(shutdown);

        tokio::select! {
            result = connection.as_mut() => {
                if let Err(error) = result {
                    tracing::debug!(error = ?error, %addr, "HTTP/1 connection finished with error");
                }
            }
            () = &mut shutdown => {
                connection.as_mut().graceful_shutdown();
                if let Err(error) = connection.as_mut().await {
                    tracing::debug!(error = ?error, %addr, "HTTP/1 connection finished with error after graceful shutdown");
                }
            }
        }
    }
}

fn is_connection_accept_error(error: &std::io::Error) -> bool {
    matches!(
        error.kind(),
        std::io::ErrorKind::ConnectionRefused
            | std::io::ErrorKind::ConnectionAborted
            | std::io::ErrorKind::ConnectionReset
    )
}

fn build_tls_acceptor(tls: &TlsConfig) -> AppResult<TlsAcceptor> {
    validate_http_tls_config(tls)?;
    let cert_file = tls.cert_file.as_deref().ok_or_else(|| {
        AppError::invalid_input("tls.cert_file", "cert_file is required for HTTPS serving")
    })?;
    let key_file = tls.key_file.as_deref().ok_or_else(|| {
        AppError::invalid_input("tls.key_file", "key_file is required for HTTPS serving")
    })?;

    let certs = load_certs(cert_file)?;
    let key = load_private_key(key_file)?;
    let versions = match tls.min_version {
        TlsVersion::Tls12 => vec![&rustls::version::TLS13, &rustls::version::TLS12],
        TlsVersion::Tls13 => vec![&rustls::version::TLS13],
        _ => vec![&rustls::version::TLS13],
    };
    let mut config = rustls::ServerConfig::builder_with_protocol_versions(&versions)
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .map_err(|error| {
            AppError::new(
                ErrorCode::InvalidInput,
                format!("invalid HTTP TLS certificate/key pair: {error}"),
            )
            .with_cause(error)
        })?;
    config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
    Ok(TlsAcceptor::from(Arc::new(config)))
}

fn load_certs(path: &str) -> AppResult<Vec<CertificateDer<'static>>> {
    let certs = CertificateDer::pem_file_iter(path)
        .map_err(|error| {
            AppError::new(
                ErrorCode::InvalidInput,
                format!("failed to load HTTP TLS certificate file '{path}': {error}"),
            )
            .with_cause(error)
        })?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| {
            AppError::new(
                ErrorCode::InvalidInput,
                format!("failed to parse HTTP TLS certificate file '{path}': {error}"),
            )
            .with_cause(error)
        })?;
    if certs.is_empty() {
        return Err(AppError::invalid_input(
            "tls.cert_file",
            "certificate file must contain at least one certificate",
        ));
    }
    Ok(certs)
}

fn load_private_key(path: &str) -> AppResult<PrivateKeyDer<'static>> {
    PrivateKeyDer::from_pem_file(path).map_err(|error| {
        AppError::new(
            ErrorCode::InvalidInput,
            format!("failed to load HTTP TLS key file '{path}': {error}"),
        )
        .with_cause(error)
    })
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
            version: rskit_version::package_version(),
        })
    }

    Router::new().route("/healthz", get(healthz_handler))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use axum::{http::Request, routing::get};
    use rskit_bootstrap::{Component, Registry};
    use rskit_http::CorsPolicy;
    use rskit_security::TransportSecurity;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tower::ServiceExt;

    use super::*;

    fn local_config() -> HttpServerConfig {
        HttpServerConfig {
            host: "127.0.0.1".to_string(),
            port: 0,
            request_timeout: Duration::from_secs(1),
            ..Default::default()
        }
    }

    #[tokio::test]
    async fn builder_applies_baseline_layers_to_application_routes() {
        let server = HttpServerBuilder::new(local_config(), CancellationToken::new())
            .with_router(Router::new().route("/", get(|| async { "ok" })))
            .with_security_headers_config(
                SecurityHeadersConfig::default()
                    .with_transport_security(TransportSecurity::AllowInsecureLocal),
            )
            .expect("configure security headers")
            .with_request_id()
            .with_tracing()
            .with_body_limit()
            .with_timeout()
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

    #[tokio::test]
    async fn lifecycle_sets_local_address_and_cancels_shutdown() {
        let server = HttpServerBuilder::new(local_config(), CancellationToken::new())
            .build()
            .expect("build server");

        assert_eq!(server.bind_addr(), "127.0.0.1:0");
        assert!(server.local_addr().is_none());
        assert!(server.health().is_healthy());

        server.start().await.expect("start http server");
        assert!(server.local_addr().is_some());
        server.stop().await.expect("stop http server");
    }

    #[tokio::test]
    async fn local_http_listener_serves_requests_and_rejects_double_start() {
        let server = HttpServerBuilder::new(local_config(), CancellationToken::new())
            .with_router(Router::new().route("/ping", get(|| async { "pong" })))
            .build()
            .expect("build server");

        server.start().await.expect("start http server");
        let second_start = server.start().await.unwrap_err();
        assert_eq!(second_start.code(), ErrorCode::Internal);
        assert!(second_start.message().contains("already started"));

        let addr = server.local_addr().expect("local address");
        let mut stream = tokio::net::TcpStream::connect(addr)
            .await
            .expect("connect to local server");
        stream
            .write_all(b"GET /ping HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
            .await
            .expect("write request");
        let mut response = String::new();
        tokio::time::timeout(Duration::from_secs(2), stream.read_to_string(&mut response))
            .await
            .expect("response read timed out")
            .expect("read response");

        assert!(response.starts_with("HTTP/1.1 200 OK"), "{response}");
        assert!(response.contains("pong"), "{response}");
        server.stop().await.expect("stop http server");
    }

    #[tokio::test]
    async fn local_http_listener_serves_http1_when_h2c_disabled() {
        let mut config = local_config();
        config.enable_h2c = false;
        let server = HttpServerBuilder::new(config, CancellationToken::new())
            .with_router(Router::new().route("/http1", get(|| async { "ok" })))
            .build()
            .expect("build server");

        server.start().await.expect("start http1 server");
        let addr = server.local_addr().expect("local address");
        let mut stream = tokio::net::TcpStream::connect(addr)
            .await
            .expect("connect to local server");
        stream
            .write_all(b"GET /http1 HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
            .await
            .expect("write request");
        let mut response = String::new();
        tokio::time::timeout(Duration::from_secs(2), stream.read_to_string(&mut response))
            .await
            .expect("response read timed out")
            .expect("read response");

        assert!(response.starts_with("HTTP/1.1 200 OK"), "{response}");
        assert!(response.contains("ok"), "{response}");
        server.stop().await.expect("stop http1 server");
    }

    #[tokio::test]
    async fn health_routers_report_registry_and_liveness() {
        let registry = Arc::new(Registry::new());
        let health = health_router(registry);
        let response = health
            .oneshot(
                Request::builder()
                    .uri("/health")
                    .body(Body::empty())
                    .expect("health request"),
            )
            .await
            .expect("health response");
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("health body");
        assert_eq!(&body[..], b"[]");

        let liveness = healthz_router();
        let response = liveness
            .oneshot(
                Request::builder()
                    .uri("/healthz")
                    .body(Body::empty())
                    .expect("healthz request"),
            )
            .await
            .expect("healthz response");
        assert_eq!(response.status(), StatusCode::OK);
        let body = axum::body::to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("healthz body");
        let body = String::from_utf8_lossy(&body);
        assert!(body.contains("\"status\":\"ok\""));
        assert!(body.contains("\"version\""));
    }

    #[test]
    fn tls_acceptor_rejects_missing_certificate_paths() {
        let tls = TlsConfig::default();

        let error = match build_tls_acceptor(&tls) {
            Ok(_) => panic!("missing TLS files should be rejected"),
            Err(error) => error,
        };

        assert_eq!(error.code(), ErrorCode::InvalidInput);
        assert!(error.message().contains("cert_file"));
    }

    #[test]
    fn tls_acceptor_rejects_missing_key_path_after_cert_path() {
        let tls = TlsConfig {
            cert_file: Some("missing-cert.pem".to_string()),
            ..Default::default()
        };

        let error = match build_tls_acceptor(&tls) {
            Ok(_) => panic!("missing key file should be rejected before reading files"),
            Err(error) => error,
        };

        assert_eq!(error.code(), ErrorCode::InvalidInput);
        assert!(error.message().contains("key_file"));
    }

    #[test]
    fn tls_loader_reports_missing_certificate_and_key_files() {
        let cert_error = load_certs("missing-cert.pem").unwrap_err();
        assert_eq!(cert_error.code(), ErrorCode::InvalidInput);
        assert!(
            cert_error
                .message()
                .contains("failed to load HTTP TLS certificate")
        );

        let key_error = load_private_key("missing-key.pem").unwrap_err();
        assert_eq!(key_error.code(), ErrorCode::InvalidInput);
        assert!(key_error.message().contains("failed to load HTTP TLS key"));
    }

    #[test]
    fn connection_accept_errors_are_classified_as_transient() {
        for kind in [
            std::io::ErrorKind::ConnectionRefused,
            std::io::ErrorKind::ConnectionAborted,
            std::io::ErrorKind::ConnectionReset,
        ] {
            let error = std::io::Error::from(kind);
            assert!(is_connection_accept_error(&error));
        }
        assert!(!is_connection_accept_error(&std::io::Error::from(
            std::io::ErrorKind::PermissionDenied
        )));
    }
}
