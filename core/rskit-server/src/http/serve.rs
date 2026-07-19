use std::future::Future;
use std::net::SocketAddr;
use std::pin::pin;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::body::Body;
use axum::serve::Listener;
use hyper::body::Incoming;
use hyper_util::rt::{TokioExecutor, TokioIo, TokioTimer};
use hyper_util::server::conn::auto::Builder as HyperBuilder;
use hyper_util::service::TowerToHyperService;
use tokio::io::{AsyncRead, AsyncWrite};
use tokio::net::TcpListener;
use tokio_rustls::TlsAcceptor;
use tokio_util::sync::CancellationToken;
use tower::ServiceExt;

use crate::http_config::HttpServerConfig;

pub(super) async fn serve_listener<L>(
    mut listener: L,
    context: ConnectionContext,
    cancel: CancellationToken,
) where
    L: Listener<Addr = SocketAddr> + Send + 'static,
    L::Io: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    loop {
        tokio::select! {
            () = cancel.cancelled() => break,
            (io, addr) = listener.accept() => {
                let context = context.clone();
                let shutdown = cancel.clone().cancelled_owned();
                tokio::spawn(async move {
                    serve_connection(io, addr, context, shutdown).await;
                });
            }
        }
    }
}

pub(super) async fn serve_tls_listener(
    listener: TcpListener,
    acceptor: TlsAcceptor,
    context: ConnectionContext,
    cancel: CancellationToken,
) {
    loop {
        tokio::select! {
            () = cancel.cancelled() => break,
            result = listener.accept() => {
                match result {
                    Ok((stream, addr)) => {
                        let acceptor = acceptor.clone();
                        let context = context.clone();
                        let shutdown = cancel.clone().cancelled_owned();
                        tokio::spawn(async move {
                            match tokio::time::timeout(context.config.read_timeout, acceptor.accept(stream)).await {
                                Ok(Ok(stream)) => {
                                    serve_connection(stream, addr, context, shutdown).await;
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
                        tokio::select! {
                            () = cancel.cancelled() => break,
                            () = tokio::time::sleep(Duration::from_secs(1)) => {}
                        }
                    }
                }
            }
        }
    }
}

/// Shared inputs for serving an accepted connection: the router, server config,
/// and whether to negotiate HTTP/2.
#[derive(Clone)]
pub(super) struct ConnectionContext {
    pub(super) router: Router,
    pub(super) config: Arc<HttpServerConfig>,
    pub(super) allow_h2: bool,
}

impl ConnectionContext {
    pub(super) fn new(router: Router, config: Arc<HttpServerConfig>, allow_h2: bool) -> Self {
        Self {
            router,
            config,
            allow_h2,
        }
    }
}

async fn serve_connection<I, F>(io: I, addr: SocketAddr, context: ConnectionContext, shutdown: F)
where
    I: AsyncRead + AsyncWrite + Unpin + Send + 'static,
    F: Future<Output = ()> + Send + 'static,
{
    let ConnectionContext {
        router,
        config,
        allow_h2,
    } = context;
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

#[cfg(test)]
mod tests {
    use super::is_connection_accept_error;

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
