//! Shutdown backstop that reaps every tracked child when a handle trips.

use std::sync::Arc;

use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::warn;

use super::registry::LiveChildRegistry;
use crate::command::LifecyclePolicy;
use crate::{AppError, AppResult};

/// Maximum number of targets terminated concurrently during a fan-out.
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

/// Terminate every registered target through its owned, reuse-proof identity.
///
/// Each pass atomically *claims* every currently-live entry before signalling
/// (so a concurrent waiter cannot reap-and-recycle a pid underneath the delayed
/// escalation), terminates the claimed targets with bounded concurrency, and
/// then either removes a confirmed-terminated target or marks a deliberate
/// survivor (`kill_after_grace = false`) so it stays registered for a later
/// attempt. The pass repeats until no live entry remains, so a child registered
/// while the fan-out is draining is still caught; because claiming, registration,
/// and the drained check all take the registry lock, no late registration is
/// lost. Targets already reaped cooperatively are absent, so the fan-out is
/// idempotent and shutting an empty registry is a clean no-op.
pub(super) async fn fan_out_shutdown(
    registry: &LiveChildRegistry,
    policy: LifecyclePolicy,
) -> AppResult<()> {
    registry.start_shutdown();
    let semaphore = Arc::new(tokio::sync::Semaphore::new(SHUTDOWN_FANOUT));
    loop {
        let batch = registry.claim_live();
        let mut tasks = Vec::with_capacity(batch.len());
        for (id, target) in batch {
            let permit = Arc::clone(&semaphore)
                .acquire_owned()
                .await
                .map_err(AppError::internal)?;
            tasks.push(tokio::spawn(async move {
                let _permit = permit;
                let outcome = target.terminate(policy).await;
                (id, outcome)
            }));
        }
        for task in tasks {
            let (id, outcome) = task.await.map_err(AppError::internal)?;
            if outcome.is_terminated() {
                registry.remove(id);
            } else {
                registry.mark_survived(id);
            }
        }
        if registry.finish_shutdown_if_drained() {
            return Ok(());
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::supervisor::registry::LiveChildRegistry;
    use crate::supervisor::target::{OwnedChild, OwnedTarget};
    use std::process::Stdio;
    use std::time::Duration;

    fn spawn_tokio(script: &str) -> tokio::process::Child {
        tokio::process::Command::new("/bin/sh")
            .args(["-c", script])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("child spawns")
    }

    async fn poll_until_gone(target: &OwnedTarget) {
        for _ in 0..500 {
            if !target.is_alive() {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        panic!("target was not reaped in time");
    }

    /// A cooperative child registered in the registry is signalled, reaped, and
    /// removed by the fan-out.
    #[tokio::test]
    async fn fan_out_reaps_registered_child() {
        let registry = Arc::new(LiveChildRegistry::default());
        let child = spawn_tokio("sleep 30");
        let pid = child.id().expect("live pid");
        let guard = registry.register(pid, false);
        registry.attach_child(guard.entry_id_for_test(), OwnedChild::Tokio(child));

        let policy = LifecyclePolicy::default().with_grace_period(Duration::from_millis(50));
        fan_out_shutdown(&registry, policy).await.expect("fan-out");

        assert_eq!(registry.len(), 0, "the reaped child is removed");
    }

    /// A child that ignores `SIGTERM` under `kill_after_grace = false` is never
    /// force-killed by the fan-out and stays registered (owned) for a later
    /// escalation, rather than being silently dropped.
    #[tokio::test]
    async fn fan_out_retains_kill_after_grace_false_survivor() {
        let registry = Arc::new(LiveChildRegistry::default());
        let child = spawn_tokio("trap '' TERM; sleep 30");
        let pid = child.id().expect("live pid");
        let guard = registry.register(pid, false);
        registry.attach_child(guard.entry_id_for_test(), OwnedChild::Tokio(child));

        let policy = LifecyclePolicy {
            kill_after_grace: false,
            ..LifecyclePolicy::default()
        }
        .with_grace_period(Duration::from_millis(50));
        fan_out_shutdown(&registry, policy).await.expect("fan-out");

        assert_eq!(
            registry.len(),
            1,
            "the surviving child is retained, not dropped"
        );

        // A follow-up shutdown with escalation enabled reaps it, proving
        // ownership was never lost.
        let kill = LifecyclePolicy::default().with_grace_period(Duration::from_millis(50));
        fan_out_shutdown(&registry, kill).await.expect("fan-out");
        assert_eq!(registry.len(), 0, "escalation later reaps the survivor");
    }

    /// The registry drains only once every live child is claimed and reaped, so a
    /// child registered during the pass is still reaped.
    #[tokio::test]
    async fn fan_out_drains_all_registered_children() {
        let registry = Arc::new(LiveChildRegistry::default());
        let mut guards = Vec::new();
        let mut targets = Vec::new();
        for _ in 0..3 {
            let child = spawn_tokio("sleep 30");
            let pid = child.id().expect("live pid");
            let guard = registry.register(pid, false);
            let id = guard.entry_id_for_test();
            let target = registry.target_for_test(id).expect("target");
            registry.attach_child(id, OwnedChild::Tokio(child));
            guards.push(guard);
            targets.push(target);
        }

        let policy = LifecyclePolicy::default().with_grace_period(Duration::from_millis(50));
        fan_out_shutdown(&registry, policy).await.expect("fan-out");

        assert_eq!(registry.len(), 0);
        for target in &targets {
            poll_until_gone(target).await;
        }
        drop(guards);
    }
}
