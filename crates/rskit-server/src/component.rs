use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use tokio::sync::oneshot;
use tokio_util::sync::CancellationToken;

use rskit_bootstrap::component::{Component, Health, HealthStatus};
use rskit_errors::{AppError, AppResult};

use crate::config::GrpcServerConfig;

// ---------------------------------------------------------------------------
// GrpcServer
// ---------------------------------------------------------------------------

/// A running gRPC server managed as a bootstrap [`Component`].
///
/// `GrpcServerBuilder::build()` produces this; it is then registered with
/// [`rskit_bootstrap::Registry`] so the app lifecycle starts and stops it.
pub struct GrpcServer {
    name: String,
    config: GrpcServerConfig,
    // The tonic Router is type-erased once built. We store it as a boxed future
    // factory so we can re-create it on start (components may be started once).
    start_fn: Arc<dyn Fn(SocketAddr, CancellationToken) -> tokio::task::JoinHandle<()> + Send + Sync>,
    cancel: CancellationToken,
    handle: parking_lot::Mutex<Option<tokio::task::JoinHandle<()>>>,
}

impl GrpcServer {
    pub(crate) fn new(
        name: String,
        config: GrpcServerConfig,
        start_fn: Arc<dyn Fn(SocketAddr, CancellationToken) -> tokio::task::JoinHandle<()> + Send + Sync>,
    ) -> Self {
        Self {
            name,
            config,
            start_fn,
            cancel: CancellationToken::new(),
            handle: parking_lot::Mutex::new(None),
        }
    }
}

#[async_trait::async_trait]
impl Component for GrpcServer {
    fn name(&self) -> &str {
        &self.name
    }

    async fn start(&self) -> AppResult<()> {
        let addr: SocketAddr = self.config.addr().parse().map_err(|e: std::net::AddrParseError| {
            AppError::internal(format!("invalid gRPC address '{}': {}", self.config.addr(), e))
        })?;

        tracing::info!(component = %self.name, addr = %addr, "starting gRPC server");

        let cancel = self.cancel.clone();
        let jh = (self.start_fn)(addr, cancel);

        *self.handle.lock() = Some(jh);

        Ok(())
    }

    async fn stop(&self) -> AppResult<()> {
        tracing::info!(component = %self.name, "stopping gRPC server");
        self.cancel.cancel();

        let handle = self.handle.lock().take();
        if let Some(jh) = handle {
            // Give the server a moment to drain connections.
            let _ = tokio::time::timeout(Duration::from_secs(10), jh).await;
        }

        Ok(())
    }

    fn health(&self) -> Health {
        let running = self.handle.lock().as_ref().map(|h| !h.is_finished()).unwrap_or(false);
        if running {
            Health::healthy(&self.name)
        } else {
            Health::new(&self.name, HealthStatus::Unhealthy, Some("server not running".into()))
        }
    }
}
