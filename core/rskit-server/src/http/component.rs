use std::net::SocketAddr;
use std::sync::Arc;

use async_trait::async_trait;
use axum::Router;
use parking_lot::Mutex;
use rskit_bootstrap::{Component, Health};
use rskit_errors::{AppError, AppResult, ErrorCode};
use tokio::net::TcpListener;
use tokio_util::sync::CancellationToken;

use super::serve::{ConnectionContext, serve_listener, serve_tls_listener};
use super::tls::build_tls_acceptor;
use crate::http_config::HttpServerConfig;

/// HTTP server that implements the [`Component`] lifecycle.
pub struct HttpServer {
    pub(super) config: Arc<HttpServerConfig>,
    pub(super) cancel: CancellationToken,
    pub(super) router: Arc<tokio::sync::Mutex<Option<Router>>>,
    pub(super) local_addr: Arc<Mutex<Option<SocketAddr>>>,
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
                let context = ConnectionContext::new(router, Arc::clone(&config), true);
                serve_tls_listener(listener, acceptor, context, cancel).await;
            } else {
                tracing::info!(addr = %actual_addr, "HTTP server listening");
                let context =
                    ConnectionContext::new(router, Arc::clone(&config), config.enable_h2c);
                serve_listener(listener, context, cancel).await;
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

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use axum::Router;
    use axum::routing::get;
    use rskit_bootstrap::Component;
    use rskit_errors::ErrorCode;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio_util::sync::CancellationToken;

    use crate::http::HttpServerBuilder;
    use crate::http::test_support::local_config;

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
}
