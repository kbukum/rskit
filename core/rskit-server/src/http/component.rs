use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

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
    pub(super) serve_handle: Arc<Mutex<Option<tokio::task::JoinHandle<bool>>>>,
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
        let handle = tokio::spawn(async move {
            if let Some(acceptor) = tls_acceptor {
                tracing::info!(addr = %actual_addr, "HTTPS server listening");
                let context = ConnectionContext::new(router, Arc::clone(&config), true);
                serve_tls_listener(listener, acceptor, context, cancel).await
            } else {
                tracing::info!(addr = %actual_addr, "HTTP server listening");
                let context =
                    ConnectionContext::new(router, Arc::clone(&config), config.enable_h2c);
                serve_listener(listener, context, cancel).await
            }
        });
        *self.serve_handle.lock() = Some(handle);

        Ok(())
    }

    async fn stop(&self) -> AppResult<()> {
        self.cancel.cancel();
        let Some(handle) = self.serve_handle.lock().take() else {
            return Ok(());
        };
        // The serve loop drains in-flight connections bounded by `shutdown_timeout`; allow a small
        // slack before abandoning the wait so `stop` still returns promptly. `saturating_add`
        // keeps an untrusted `shutdown_timeout` (e.g. `u64::MAX` seconds) from overflowing.
        let deadline = self
            .config
            .shutdown_timeout
            .saturating_add(Duration::from_secs(1));
        let abort = handle.abort_handle();
        match tokio::time::timeout(deadline, handle).await {
            Ok(Ok(true)) => Ok(()),
            Ok(Ok(false)) => Err(AppError::new(
                ErrorCode::Timeout,
                "HTTP server did not drain in-flight connections within shutdown timeout",
            )),
            Ok(Err(join_error)) => Err(AppError::new(
                ErrorCode::Internal,
                format!("HTTP serve task ended abnormally: {join_error}"),
            )
            .with_cause(join_error)),
            Err(_) => {
                abort.abort();
                Err(AppError::new(
                    ErrorCode::Timeout,
                    "HTTP server shutdown exceeded its deadline",
                ))
            }
        }
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
    use rskit_security::TlsConfig;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio_util::sync::CancellationToken;

    use crate::http::HttpServerBuilder;
    use crate::http::test_support::local_config;

    fn testdata(name: &str) -> String {
        format!("{}/testdata/{name}", env!("CARGO_MANIFEST_DIR"))
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
    async fn https_listener_completes_tls_handshake_and_serves_request() {
        use std::sync::Arc;

        use rustls::RootCertStore;
        use rustls::pki_types::{CertificateDer, ServerName, pem::PemObject};
        use tokio_rustls::TlsConnector;

        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

        let mut config = local_config();
        config.tls = Some(TlsConfig {
            cert_file: Some(testdata("cert.pem")),
            key_file: Some(testdata("key.pem")),
            ..Default::default()
        });
        let server = HttpServerBuilder::new(config, CancellationToken::new())
            .with_router(Router::new().route("/secure", get(|| async { "encrypted" })))
            .build()
            .expect("build https server");

        server.start().await.expect("start https server");
        let addr = server.local_addr().expect("local address");

        let mut roots = RootCertStore::empty();
        roots
            .add(CertificateDer::from_pem_file(testdata("cert.pem")).expect("load test cert"))
            .expect("trust test cert");
        let client_config = rustls::ClientConfig::builder()
            .with_root_certificates(roots)
            .with_no_client_auth();
        let connector = TlsConnector::from(Arc::new(client_config));
        let server_name = ServerName::try_from("localhost").expect("server name");

        let tcp = tokio::net::TcpStream::connect(addr)
            .await
            .expect("connect to https server");
        let mut stream = connector
            .connect(server_name, tcp)
            .await
            .expect("tls handshake");
        stream
            .write_all(b"GET /secure HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
            .await
            .expect("write request");
        let mut response = String::new();
        tokio::time::timeout(Duration::from_secs(2), stream.read_to_string(&mut response))
            .await
            .expect("response read timed out")
            .expect("read response");

        assert!(response.starts_with("HTTP/1.1 200 OK"), "{response}");
        assert!(response.contains("encrypted"), "{response}");
        server.stop().await.expect("stop https server");
    }

    #[tokio::test]
    async fn start_reports_bind_failure_for_address_in_use() {
        let occupied = tokio::net::TcpListener::bind("127.0.0.1:0")
            .await
            .expect("bind probe listener");
        let taken = occupied.local_addr().expect("probe local address");

        let mut config = local_config();
        config.port = taken.port();
        let server = HttpServerBuilder::new(config, CancellationToken::new())
            .build()
            .expect("build server");

        let error = server.start().await.expect_err("bind should fail");
        assert_eq!(error.code(), ErrorCode::Internal);
        assert!(
            error.message().contains("bind failed"),
            "{}",
            error.message()
        );
        assert!(server.local_addr().is_none());
    }

    #[tokio::test]
    async fn https_listener_times_out_stalled_handshake() {
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

        let mut config = local_config();
        config.read_timeout = Duration::from_millis(50);
        config.tls = Some(TlsConfig {
            cert_file: Some(testdata("cert.pem")),
            key_file: Some(testdata("key.pem")),
            ..Default::default()
        });
        let server = HttpServerBuilder::new(config, CancellationToken::new())
            .build()
            .expect("build https server");

        server.start().await.expect("start https server");
        let addr = server.local_addr().expect("local address");
        // Connect but never send a ClientHello so the handshake stalls and the
        // server's read-timeout aborts it.
        let _stream = tokio::net::TcpStream::connect(addr)
            .await
            .expect("connect to https server");
        tokio::time::sleep(Duration::from_millis(150)).await;

        server.stop().await.expect("stop https server");
    }

    async fn assert_serves_and_drains_on_shutdown(enable_h2c: bool) {
        let mut config = local_config();
        config.enable_h2c = enable_h2c;
        let server = HttpServerBuilder::new(config, CancellationToken::new())
            .with_router(Router::new().route("/keep", get(|| async { "ok" })))
            .build()
            .expect("build server");

        server.start().await.expect("start server");
        let addr = server.local_addr().expect("local address");
        let mut stream = tokio::net::TcpStream::connect(addr)
            .await
            .expect("connect to local server");
        // Keep-alive request (no `Connection: close`) leaves the server-side
        // connection idle so that stopping the server exercises graceful drain.
        stream
            .write_all(b"GET /keep HTTP/1.1\r\nHost: localhost\r\n\r\n")
            .await
            .expect("write request");

        let mut buf = vec![0u8; 1024];
        let read = tokio::time::timeout(Duration::from_secs(2), stream.read(&mut buf))
            .await
            .expect("response read timed out")
            .expect("read response");
        let head = String::from_utf8_lossy(&buf[..read]);
        assert!(head.starts_with("HTTP/1.1 200 OK"), "{head}");

        server.stop().await.expect("stop server");

        let mut rest = Vec::new();
        let _ = tokio::time::timeout(Duration::from_secs(2), stream.read_to_end(&mut rest)).await;
    }

    #[tokio::test]
    async fn h2c_connection_drains_on_graceful_shutdown() {
        assert_serves_and_drains_on_shutdown(true).await;
    }

    #[tokio::test]
    async fn http1_connection_drains_on_graceful_shutdown() {
        assert_serves_and_drains_on_shutdown(false).await;
    }

    #[tokio::test]
    async fn https_listener_survives_non_tls_client() {
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();

        let mut config = local_config();
        config.tls = Some(TlsConfig {
            cert_file: Some(testdata("cert.pem")),
            key_file: Some(testdata("key.pem")),
            ..Default::default()
        });
        let server = HttpServerBuilder::new(config, CancellationToken::new())
            .build()
            .expect("build https server");

        server.start().await.expect("start https server");
        let addr = server.local_addr().expect("local address");
        let mut stream = tokio::net::TcpStream::connect(addr)
            .await
            .expect("connect to https server");
        // Plain-text bytes cannot complete the TLS handshake; the server must
        // log and drop the connection without terminating the accept loop.
        stream
            .write_all(b"GET / HTTP/1.1\r\nHost: localhost\r\n\r\n")
            .await
            .expect("write plaintext request");
        let mut buf = vec![0u8; 64];
        let _ = tokio::time::timeout(Duration::from_millis(200), stream.read(&mut buf)).await;

        server.stop().await.expect("stop https server");
    }
}
