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
/// The subscription owns the watcher task that *waits* for the handle to be cancelled and then
/// launches the backstop fan-out. Dropping it stops watching for a not-yet-fired handle, but never
/// interrupts a fan-out that has already begun — once cancellation fires the reaping runs to
/// completion on its own detached task.
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
        // Detach the reaping onto its own task so that dropping the subscription (which aborts
        // only this watcher) cannot cancel a fan-out that has already started.
        tokio::spawn(async move {
            if let Err(error) = fan_out_shutdown(&registry, policy).await {
                warn!("supervisor shutdown backstop failed: {error}");
            }
        });
    });
    ShutdownSubscription { watcher }
}

/// Terminate every currently registered group with bounded concurrency, then unregister it.
///
/// Fan-out is capped at [`SHUTDOWN_FANOUT`] concurrent terminations and every spawned task is
/// drained before this returns. Groups already reaped cooperatively are absent from the
/// snapshot, so the fan-out is idempotent and shutting an empty registry is a clean no-op. A
/// target is unregistered only once termination is confirmed (or a force kill was issued); a
/// child that deliberately survives with escalation disabled stays registered for a later retry.
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
            let terminated = terminate_registered_pid(child.pid, child.process_group, policy).await;
            (child.pid, terminated)
        }));
    }
    for task in tasks {
        let (pid, terminated) = task.await.map_err(AppError::internal)?;
        if terminated {
            registry.unregister(pid);
        }
    }
    Ok(())
}
