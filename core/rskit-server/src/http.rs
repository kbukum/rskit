use std::fs::File;
use std::future::Future;
use std::io::BufReader;
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
use rskit_bootstrap::{Component, Health, Registry};
use rskit_errors::{AppError, AppResult, ErrorCode};
use rskit_http::{SecurityHeadersConfig, SecurityHeadersLayer};
use rskit_security::{TlsConfig, TlsVersion};
use rustls::pki_types::{CertificateDer, PrivateKeyDer};
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::{TcpListener, TcpStream};
use tokio_rustls::{TlsAcceptor, server::TlsStream};
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
    ///
    /// # Errors
    /// Returns an error when the configured CORS policy contains invalid origins,
    /// methods, headers, or max-age values.
    #[must_use = "builder methods return a new builder; use the returned value"]
    pub fn with_cors(mut self) -> AppResult<Self> {
        if let Some(cors_cfg) = self.config.cors.as_ref() {
            let cors = cors_cfg.layer()?;
            self.router = self.router.layer(cors);
        }
        Ok(self)
    }

    /// Add automatic `X-Request-Id` injection.
    #[must_use]
    pub fn with_request_id(mut self) -> Self {
        self.router = self
            .router
            .layer(SetRequestIdLayer::x_request_id(MakeRequestUuid));
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
        let layer = SecurityHeadersLayer::new(&config)?;
        self.router = self.router.layer(layer);
        Ok(self)
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
    /// # Errors
    /// Returns an error when baseline transport middleware configuration is invalid.
    pub fn build(self) -> AppResult<HttpServer> {
        let mut builder = self
            .with_tracing()
            .with_request_id()
            .with_body_limit()
            .with_timeout();
        builder = builder.with_security_headers()?;
        builder = builder.with_cors()?;
        Ok(HttpServer {
            config: Arc::new(builder.config),
            cancel: builder.cancel,
            router: Arc::new(tokio::sync::Mutex::new(Some(
                builder.middleware.apply(builder.router),
            ))),
        })
    }

    /// Add the configured request body limit.
    #[must_use]
    pub fn with_body_limit(mut self) -> Self {
        self.router = self
            .router
            .layer(DefaultBodyLimit::max(self.config.max_body_bytes));
        self
    }

    /// Add the configured request timeout.
    #[must_use]
    pub fn with_timeout(mut self) -> Self {
        self.router = self.router.layer(TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            self.config.request_timeout,
        ));
        self
    }
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

        let cancel = self.cancel.clone();
        let config = Arc::clone(&self.config);
        tokio::spawn(async move {
            let listener = match TcpListener::bind(addr).await {
                Ok(listener) => listener,
                Err(error) => {
                    tracing::error!(error = ?error, %addr, "HTTP server bind failed");
                    return;
                }
            };

            if let Some(acceptor) = tls_acceptor {
                tracing::info!(%addr, "HTTPS server listening");
                let listener = TlsListener { listener, acceptor };
                serve_listener(listener, router, Arc::clone(&config), cancel).await;
            } else {
                tracing::info!(%addr, "HTTP server listening");
                serve_listener(listener, router, Arc::clone(&config), cancel).await;
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
                    serve_connection(io, addr, router, config, shutdown).await;
                });
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
    let mut connection = pin!(builder.serve_connection_with_upgrades(io, hyper_service));
    let mut shutdown = pin!(shutdown);

    loop {
        tokio::select! {
            result = connection.as_mut() => {
                if let Err(error) = result {
                    tracing::debug!(error = ?error, %addr, "HTTP connection finished with error");
                }
                break;
            }
            () = &mut shutdown => {
                connection.as_mut().graceful_shutdown();
            }
        }
    }
}

struct TlsListener {
    listener: TcpListener,
    acceptor: TlsAcceptor,
}

impl Listener for TlsListener {
    type Io = TlsStream<TcpStream>;
    type Addr = SocketAddr;

    async fn accept(&mut self) -> (Self::Io, Self::Addr) {
        loop {
            match self.listener.accept().await {
                Ok((stream, addr)) => match self.acceptor.accept(stream).await {
                    Ok(stream) => return (stream, addr),
                    Err(error) => {
                        tracing::warn!(error = ?error, %addr, "TLS handshake failed");
                    }
                },
                Err(error) if is_connection_accept_error(&error) => {}
                Err(error) => {
                    tracing::error!(error = ?error, "HTTP TLS accept failed");
                    tokio::time::sleep(Duration::from_secs(1)).await;
                }
            }
        }
    }

    fn local_addr(&self) -> std::io::Result<Self::Addr> {
        self.listener.local_addr()
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
    let file = File::open(path).map_err(|error| {
        AppError::new(
            ErrorCode::InvalidInput,
            format!("failed to open HTTP TLS certificate file '{path}': {error}"),
        )
        .with_cause(error)
    })?;
    let mut reader = BufReader::new(file);
    let certs = rustls_pemfile::certs(&mut reader)
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
    let file = File::open(path).map_err(|error| {
        AppError::new(
            ErrorCode::InvalidInput,
            format!("failed to open HTTP TLS key file '{path}': {error}"),
        )
        .with_cause(error)
    })?;
    let mut reader = BufReader::new(file);
    rustls_pemfile::private_key(&mut reader)
        .map_err(|error| {
            AppError::new(
                ErrorCode::InvalidInput,
                format!("failed to parse HTTP TLS key file '{path}': {error}"),
            )
            .with_cause(error)
        })?
        .ok_or_else(|| {
            AppError::invalid_input(
                "tls.key_file",
                "key file must contain one PKCS#8, PKCS#1, or SEC1 private key",
            )
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
