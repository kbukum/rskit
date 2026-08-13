//! Shutdown backstop that reaps every tracked child when a handle trips.

use std::sync::Arc;

use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::warn;

use super::registry::LiveChildRegistry;
use super::termination::terminate_registered_pid;
use crate::command::LifecyclePolicy;
use crate::{AppError, AppResult};

/// Maximum number of groups terminated concurrently during a fan-out.
const SHUTDOWN_FANOUT: usize = 32;

/// Backstop subscription bound to a caller-owned shutdown handle.
///
/// The subscription owns the watcher task that reaps every tracked group once the handle
/// is cancelled. Dropping it stops watching without disturbing any reaping already underway.
#[derive(Debug)]
pub struct ShutdownSubscription {
    watcher: JoinHandle<()>,
}

impl Drop for ShutdownSubscription {
    fn drop(&mut self) {
        self.watcher.abort();
    }
}

/// Spawn the watcher that runs the backstop fan-out when `token` is cancelled.
pub(super) fn subscribe(
    registry: Arc<LiveChildRegistry>,
    policy: LifecyclePolicy,
    token: CancellationToken,
) -> ShutdownSubscription {
    let watcher = tokio::spawn(async move {
        token.cancelled().await;
        if let Err(error) = fan_out_shutdown(&registry, policy).await {
            warn!("supervisor shutdown backstop failed: {error}");
        }
    });
    ShutdownSubscription { watcher }
}

/// Terminate every currently registered group with bounded concurrency, then unregister it.
///
/// Fan-out is capped at [`SHUTDOWN_FANOUT`] concurrent terminations and every spawned task is
/// drained before this returns. Groups already reaped cooperatively are absent from the
/// snapshot, so the fan-out is idempotent and shutting an empty registry is a clean no-op.
pub(super) async fn fan_out_shutdown(
    registry: &LiveChildRegistry,
    policy: LifecyclePolicy,
) -> AppResult<()> {
    let children = registry.snapshot();
    let semaphore = Arc::new(tokio::sync::Semaphore::new(SHUTDOWN_FANOUT));
    let mut tasks = Vec::with_capacity(children.len());
    for child in children {
        let permit = Arc::clone(&semaphore)
            .acquire_owned()
            .await
            .map_err(AppError::internal)?;
        tasks.push(tokio::spawn(async move {
            let _permit = permit;
            terminate_registered_pid(child.pid, child.process_group, policy).await;
            child.pid
        }));
    }
    for task in tasks {
        let pid = task.await.map_err(AppError::internal)?;
        registry.unregister(pid);
    }
    Ok(())
}
