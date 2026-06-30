use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use tokio_util::sync::CancellationToken;

use rskit_bootstrap::{Component, Health};
use rskit_errors::{AppError, AppResult};
use rskit_stream::SpawnedTask;

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
    start_fn:
        Arc<dyn Fn(SocketAddr, CancellationToken) -> tokio::task::JoinHandle<()> + Send + Sync>,
    task: Mutex<Option<SpawnedTask>>,
}

impl GrpcServer {
    pub(crate) fn new(
        name: String,
        config: GrpcServerConfig,
        start_fn: Arc<
            dyn Fn(SocketAddr, CancellationToken) -> tokio::task::JoinHandle<()> + Send + Sync,
        >,
    ) -> Self {
        Self {
            name,
            config,
            start_fn,
            task: Mutex::new(None),
        }
    }
}

#[async_trait::async_trait]
impl Component for GrpcServer {
    fn name(&self) -> &str {
        &self.name
    }

    async fn start(&self) -> AppResult<()> {
        let addr: SocketAddr =
            self.config
                .addr()
                .parse()
                .map_err(|e: std::net::AddrParseError| {
                    AppError::new(
                        rskit_errors::ErrorCode::Internal,
                        format!("invalid gRPC address '{}': {}", self.config.addr(), e),
                    )
                })?;

        tracing::info!(component = %self.name, addr = %addr, "starting gRPC server");

        let cancel = CancellationToken::new();
        let jh = (self.start_fn)(addr, cancel.clone());

        *self.task.lock() = Some(SpawnedTask::from_parts(cancel, jh));

        Ok(())
    }

    async fn stop(&self) -> AppResult<()> {
        tracing::info!(component = %self.name, "stopping gRPC server");

        let task = self.task.lock().take();
        if let Some(task) = task {
            task.shutdown(Duration::from_secs(10)).await;
        }

        Ok(())
    }

    fn health(&self) -> Health {
        let running = self
            .task
            .lock()
            .as_ref()
            .map(|t| !t.is_finished())
            .unwrap_or(false);
        if running {
            Health::healthy(&self.name)
        } else {
            Health::unhealthy(&self.name, "server not running")
        }
    }
}
