//! Shutdown backstop that reaps every tracked child when a handle trips.

use std::sync::Arc;

use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::warn;

use super::registry::LiveChildRegistry;
use super::target::TargetOutcome;
use crate::command::LifecyclePolicy;
use crate::{AppError, AppResult, ErrorCode};

/// Maximum number of targets terminated concurrently during a fan-out.
const SHUTDOWN_FANOUT: usize = 32;

struct ClaimGuard<'a> {
    registry: &'a LiveChildRegistry,
    outstanding: Vec<(u64, Arc<super::target::OwnedTarget>)>,
}

impl<'a> ClaimGuard<'a> {
    fn new(
        registry: &'a LiveChildRegistry,
        claimed: &[(u64, Arc<super::target::OwnedTarget>)],
    ) -> Self {
        Self {
            registry,
            outstanding: claimed.to_vec(),
        }
    }

    fn complete(&mut self, id: u64) {
        self.outstanding.retain(|(candidate, _)| *candidate != id);
    }
}

impl Drop for ClaimGuard<'_> {
    fn drop(&mut self) {
        self.registry.release_claims(&self.outstanding);
    }
}

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

/// Terminate every registered target through its owned identity — reuse-proof
/// for the leader (pidfd/owned handle), best-effort for the process group.
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
    // Serialize concurrent shutdown passes: without this, a second caller could
    // observe the first caller's claimed (but not yet terminated) entries as
    // "drained" and return before those children are reaped.
    let _gate = registry.shutdown_gate().await;
    registry.start_shutdown();
    let mut unsupported = 0usize;
    let mut failed = 0usize;
    loop {
        let batch = registry.claim_live(SHUTDOWN_FANOUT);
        let mut claims = ClaimGuard::new(registry, &batch);
        let mut tasks = tokio::task::JoinSet::new();
        for (id, target) in batch {
            tasks.spawn(async move { (id, target.terminate(policy).await) });
        }
        while let Some(task) = tasks.join_next().await {
            let (id, outcome) = task.map_err(AppError::internal)?;
            match outcome {
                TargetOutcome::Terminated => registry.complete_claim(id, false),
                TargetOutcome::Survived => registry.complete_claim(id, true),
                TargetOutcome::Unsupported => {
                    registry.complete_claim(id, true);
                    unsupported += 1;
                }
                TargetOutcome::Failed => {
                    registry.complete_claim(id, true);
                    failed += 1;
                }
            }
            claims.complete(id);
        }
        if registry.finish_shutdown_if_drained() {
            break;
        }
    }
    if unsupported > 0 {
        return Err(AppError::new(
            ErrorCode::Internal,
            format!(
                "process supervision shutdown cannot terminate {unsupported} target(s) on this \
                 platform: no signalling primitive and no owned child handle"
            ),
        ));
    }
    if failed > 0 {
        return Err(AppError::new(
            ErrorCode::Internal,
            format!(
                "process supervision shutdown could not confirm termination of {failed} target(s)"
            ),
        ));
    }
    Ok(())
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::supervisor::registry::LiveChildRegistry;
    use crate::supervisor::target::{OwnedChild, OwnedTarget};
    use std::process::Stdio;
    use std::time::Duration;
    use tokio::io::AsyncReadExt;

    fn spawn_tokio(script: &str) -> tokio::process::Child {
        tokio::process::Command::new("/bin/sh")
            .args(["-c", script])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("child spawns")
    }

    async fn spawn_stubborn_tokio() -> tokio::process::Child {
        let mut child = tokio::process::Command::new("/bin/sh")
            .args(["-c", "trap '' TERM; printf ready; sleep 30"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("child spawns");
        let mut ready = [0_u8; 5];
        child
            .stdout
            .as_mut()
            .expect("stdout")
            .read_exact(&mut ready)
            .await
            .expect("read readiness");
        assert_eq!(&ready, b"ready");
        child
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
        assert!(
            registry
                .attach_child(guard.entry_id_for_test(), OwnedChild::Tokio(child))
                .is_none()
        );

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
        let child = spawn_stubborn_tokio().await;
        let pid = child.id().expect("live pid");
        let guard = registry.register(pid, false);
        assert!(
            registry
                .attach_child(guard.entry_id_for_test(), OwnedChild::Tokio(child))
                .is_none()
        );

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
            assert!(
                registry
                    .attach_child(id, OwnedChild::Tokio(child))
                    .is_none()
            );
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

    /// Two concurrent shutdown passes never let one caller return while the other
    /// caller's claimed children are still terminating: the shutdown gate
    /// serializes the passes, so both observe a fully drained registry.
    #[tokio::test]
    async fn concurrent_shutdowns_do_not_return_before_children_terminate() {
        let registry = Arc::new(LiveChildRegistry::default());
        let mut guards = Vec::new();
        let mut targets = Vec::new();
        for _ in 0..4 {
            let child = spawn_tokio("sleep 30");
            let pid = child.id().expect("live pid");
            let guard = registry.register(pid, false);
            let id = guard.entry_id_for_test();
            targets.push(registry.target_for_test(id).expect("target"));
            assert!(
                registry
                    .attach_child(id, OwnedChild::Tokio(child))
                    .is_none()
            );
            guards.push(guard);
        }

        let policy = LifecyclePolicy::default().with_grace_period(Duration::from_millis(50));
        let a = {
            let registry = Arc::clone(&registry);
            tokio::spawn(async move { fan_out_shutdown(&registry, policy).await })
        };
        let b = {
            let registry = Arc::clone(&registry);
            tokio::spawn(async move { fan_out_shutdown(&registry, policy).await })
        };
        a.await.expect("join a").expect("fan-out a");
        b.await.expect("join b").expect("fan-out b");

        assert_eq!(registry.len(), 0, "both passes leave the registry drained");
        for target in &targets {
            poll_until_gone(target).await;
        }
        drop(guards);
    }

    /// Cancelling a public shutdown future restores every unfinished claim, so
    /// a later shutdown can retry and reap the same children.
    #[tokio::test(start_paused = true)]
    async fn cancelled_shutdown_releases_claims_for_retry() {
        let registry = Arc::new(LiveChildRegistry::default());
        let child = spawn_stubborn_tokio().await;
        let pid = child.id().expect("live pid");
        let guard = registry.register(pid, false);
        assert!(
            registry
                .attach_child(guard.entry_id_for_test(), OwnedChild::Tokio(child))
                .is_none()
        );

        let slow = LifecyclePolicy::default().with_grace_period(Duration::from_secs(30));
        let first = {
            let registry = Arc::clone(&registry);
            tokio::spawn(async move { fan_out_shutdown(&registry, slow).await })
        };
        tokio::task::yield_now().await;
        first.abort();
        assert!(
            first
                .await
                .expect_err("shutdown is cancelled")
                .is_cancelled()
        );

        tokio::time::resume();
        let retry = LifecyclePolicy::default().with_grace_period(Duration::from_millis(50));
        fan_out_shutdown(&registry, retry)
            .await
            .expect("retry shutdown");

        assert_eq!(registry.len(), 0, "retry reaps the restored claim");
        drop(guard);
    }
}
