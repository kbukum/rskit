//! Process supervisor public API and child wrappers.

use std::process::{Child, Command as StdCommand};
use std::sync::Arc;

use tokio::process::{Child as TokioChild, Command as TokioCommand};

use super::registry::{LiveChildRegistry, RegistrationGuard};
use super::shutdown::{ShutdownSubscription, fan_out_shutdown, subscribe};
use crate::command::LifecyclePolicy;
use crate::process_group::{isolate, isolate_async, kill_target};
use crate::{AppError, AppResult, ErrorCode, command::spawn_error};
use tokio_util::sync::CancellationToken;
use tracing::debug;

/// Owns spawned-child lifetime for one caller scope.
///
/// A supervisor tracks each spawned child through an owned, reuse-proof identity
/// (a Linux pidfd for the direct child where available, otherwise the pid and
/// process-group id) registered with the topology the child was actually spawned
/// under. Termination and shutdown act on that identity, so a delayed
/// escalation always targets the exact original process and never a pid the OS
/// recycled. The child-owning wrapper it returns ([`SupervisedBlockingChild`] /
/// [`SupervisedAsyncChild`]) best-effort kills its child on drop — including on
/// panic unwinding — so lifetime is guaranteed while the wrapper is held. The
/// supervisor's own [`Drop`] is a backstop that force-kills and reaps any target
/// still registered when the supervisor itself is dropped. Normal run paths
/// unregister on reap through the returned registration guard; a child that
/// deliberately outlives its grace period (`kill_after_grace = false`) is handed
/// to its owned target and stays registered so a later shutdown or supervisor
/// drop can still reap it. Double cleanup is safe.
///
/// On platforms without a `kill(2)`-style primitive (non-Unix), the supervisor
/// can still tear down a child whose handle it owns, but [`shutdown`] returns an
/// error for any registered child it can neither signal nor own, rather than
/// silently reporting success. Full process-supervision guarantees therefore
/// hold on Unix; other platforms are best-effort and honest about it.
///
/// [`shutdown`]: Self::shutdown
#[derive(Debug)]
pub struct ProcessSupervisor {
    registry: Arc<LiveChildRegistry>,
    lifecycle: LifecyclePolicy,
}

impl ProcessSupervisor {
    /// Create a supervisor with the provided lifecycle policy.
    #[must_use]
    pub fn new(lifecycle: LifecyclePolicy) -> Self {
        Self {
            registry: Arc::new(LiveChildRegistry::default()),
            lifecycle,
        }
    }

    /// Spawn a blocking child and register it for supervised cleanup.
    pub fn spawn_blocking(&self, command: &mut StdCommand) -> AppResult<SupervisedBlockingChild> {
        if self.lifecycle.isolate_process_group {
            isolate(command);
        }
        let child = command
            .spawn()
            .map_err(|error| spawn_error("failed to spawn process", error))?;
        let guard = self.register_pid(child.id());
        Ok(SupervisedBlockingChild {
            child,
            guard: Some(guard),
            lifecycle: self.lifecycle,
        })
    }

    /// Spawn an async child and register it for supervised cleanup.
    pub fn spawn_async(&self, command: &mut TokioCommand) -> AppResult<SupervisedAsyncChild> {
        if self.lifecycle.isolate_process_group {
            isolate_async(command);
        }
        let child = command
            .spawn()
            .map_err(|error| spawn_error("failed to spawn process", error))?;
        let guard = self.register_pid(child.id().unwrap_or_default());
        Ok(SupervisedAsyncChild {
            child,
            guard: Some(guard),
            lifecycle: self.lifecycle,
        })
    }

    /// Register an existing child pid, capturing an owned target for it.
    ///
    /// Uses the supervisor's own lifecycle topology; for children spawned with a
    /// different policy the runners call [`register_pid_with_group`] with the
    /// actual spawn topology so the owned target signals the right identity.
    ///
    /// [`register_pid_with_group`]: Self::register_pid_with_group
    pub(crate) fn register_pid(&self, pid: u32) -> RegistrationGuard {
        self.register_pid_with_group(pid, self.lifecycle.targets_group())
    }

    /// Register an existing child pid with the topology it was actually spawned
    /// under.
    ///
    /// A supervised runner configures its spawn from its own `ProcessConfig`,
    /// which may differ from the supervisor's policy. Registering with the
    /// spawn's real `targets_group` value keeps the owned target's group
    /// signalling consistent with how the child was created, so shutdown never
    /// treats a leader-only child as a group (or vice versa).
    pub(crate) fn register_pid_with_group(
        &self,
        pid: u32,
        targets_group: bool,
    ) -> RegistrationGuard {
        self.registry.register(pid, targets_group)
    }

    /// Shut down every currently tracked child target.
    ///
    /// Shutdown fans out with bounded concurrency, atomically claims each live
    /// target, sends graceful termination through its reuse-proof identity, waits
    /// the policy grace period, escalates to `SIGKILL` when enabled, and removes
    /// the target only once its termination is confirmed. A target that survives
    /// with escalation disabled stays registered. Runners that own live child
    /// handles still perform the actual wait/reap on their normal paths.
    /// Concurrent shutdown passes are serialized, so a second caller never
    /// returns while another pass's claimed children are still terminating.
    ///
    /// # Errors
    ///
    /// Returns an error if a tracked child cannot be terminated on this platform
    /// — for example a non-Unix target with no signalling primitive and no owned
    /// child handle. The affected entries stay registered rather than being
    /// dropped as though they had exited.
    pub async fn shutdown(&self, reason: impl Into<String>) -> AppResult<()> {
        let reason = reason.into();
        debug!(reason = %reason, "supervisor shutdown requested");
        fan_out_shutdown(&self.registry, self.lifecycle).await
    }

    /// Return the number of tracked children.
    #[must_use]
    pub fn registry_len(&self) -> usize {
        self.registry.len()
    }

    /// Subscribe this supervisor to a caller-owned shutdown handle as the backstop.
    ///
    /// # Two-phase shutdown contract
    ///
    /// This wires the supervisor as the *second* phase of process shutdown behind a
    /// cooperative canceller such as [`tokio_util::sync::CancellationToken`]:
    ///
    /// 1. **Cooperative (phase one).** The caller's own tasks observe the same handle
    ///    and drain in-flight work, reaping the children they still hold. Groups reaped
    ///    this way unregister through their [`RegistrationGuard`], so the backstop never
    ///    sees them.
    /// 2. **Backstop (phase two).** When the handle is cancelled, the supervisor fans out
    ///    graceful termination to every *still-registered* target, escalates per policy, and
    ///    removes each on a confirmed reap. Reaping is idempotent, so a target already torn
    ///    down cooperatively is a clean no-op — no double-kill and no deadlock against phase one.
    /// 3. **Force-exit (escalation).** A caller's force-exit path (for example a second signal
    ///    driving `std::process::exit`) is independent of this backstop and always wins, even
    ///    while a slow target is still draining.
    ///
    /// The returned [`ShutdownSubscription`] owns the watcher task; dropping it stops watching
    /// without disturbing any reaping already in progress.
    ///
    /// # Errors
    ///
    /// Returns an error if called outside a Tokio runtime, since the watcher runs as a spawned
    /// task; this keeps a synchronous, blocking-child-capable supervisor from panicking.
    pub fn subscribe_shutdown(&self, token: CancellationToken) -> AppResult<ShutdownSubscription> {
        tokio::runtime::Handle::try_current().map_err(|error| {
            AppError::new(
                ErrorCode::Internal,
                "subscribe_shutdown must be called from within a Tokio runtime",
            )
            .with_cause(error)
        })?;
        Ok(subscribe(Arc::clone(&self.registry), self.lifecycle, token))
    }

    /// Register an externally spawned pid under this supervisor.
    #[must_use]
    pub fn track_pid(&self, pid: u32) -> RegistrationGuard {
        self.register_pid(pid)
    }

    /// Unregister an externally tracked pid.
    pub fn unregister_pid(&self, pid: u32) {
        self.registry.remove_pid(pid);
    }
}

impl Drop for ProcessSupervisor {
    fn drop(&mut self) {
        for (id, target) in self.registry.drain_targets() {
            target.kill_blocking();
            self.registry.remove(id);
        }
    }
}

/// A blocking child paired with its registry guard.
#[derive(Debug)]
pub struct SupervisedBlockingChild {
    child: Child,
    guard: Option<RegistrationGuard>,
    lifecycle: LifecyclePolicy,
}

impl Drop for SupervisedBlockingChild {
    fn drop(&mut self) {
        let _ = kill_target(self.child.id(), self.lifecycle.targets_group());
        let _ = self.child.kill();
        let _ = self.child.wait();
        if let Some(guard) = self.guard.take() {
            guard.unregister();
        }
    }
}

/// An async child paired with its registry guard.
#[derive(Debug)]
pub struct SupervisedAsyncChild {
    child: TokioChild,
    guard: Option<RegistrationGuard>,
    lifecycle: LifecyclePolicy,
}

impl Drop for SupervisedAsyncChild {
    fn drop(&mut self) {
        if let Some(pid) = self.child.id() {
            let _ = kill_target(pid, self.lifecycle.targets_group());
        }
        let _ = self.child.start_kill();
        if let Some(guard) = self.guard.take() {
            guard.unregister();
        }
    }
}
