//! Process supervisor public API and child wrappers.

use std::mem::ManuallyDrop;
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
/// A supervisor registers each spawned child by process-group id (or child pid when isolation is disabled). The child-owning wrapper it returns ([`SupervisedBlockingChild`] / [`SupervisedAsyncChild`]) is what best-effort kills its child on drop — including on panic unwinding — so lifetime is guaranteed as long as the wrapper is held. The supervisor's own [`Drop`] is a backstop that kills any target still registered when the supervisor itself is dropped (for example externally [`track_pid`](Self::track_pid)-ed pids). Normal run paths unregister on reap through the returned registration guard; double cleanup is safe.
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

    /// Register an existing child pid.
    pub(crate) fn register_pid(&self, pid: u32) -> RegistrationGuard {
        self.registry.register(pid, self.lifecycle.targets_group())
    }

    /// Shut down every currently tracked child target.
    ///
    /// Shutdown fans out with bounded concurrency, sends graceful termination, waits the policy grace period, escalates to `SIGKILL` when enabled, and unregisters the target. Runners that own child handles still perform the actual wait/reap on their normal paths.
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
    ///    graceful termination to every *still-registered* group, escalates per policy, and
    ///    unregisters each. Reaping is idempotent, so a group already torn down cooperatively
    ///    is a clean no-op — no double-kill and no deadlock against phase one.
    /// 3. **Force-exit (escalation).** A caller's force-exit path (for example a second signal
    ///    driving `std::process::exit`) is independent of this backstop and always wins, even
    ///    while a slow group is still draining.
    ///
    /// The returned [`ShutdownSubscription`] owns the watcher task; dropping it stops watching
    /// without disturbing any reaping already in progress.
    ///
    /// # Errors
    ///
    /// Returns an error if called outside a Tokio runtime, since the watcher runs as a spawned
    /// task; this keeps a synchronous, blocking-child-capable supervisor from panicking.
    pub fn subscribe_shutdown(
        &self,
        token: CancellationToken,
    ) -> AppResult<ShutdownSubscription> {
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
        self.registry.unregister(pid);
    }
}

impl Drop for ProcessSupervisor {
    fn drop(&mut self) {
        for child in self.registry.snapshot() {
            let _ = kill_target(child.pid, child.process_group);
            self.registry.unregister(child.pid);
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

impl SupervisedBlockingChild {
    /// Split the wrapper into the child handle and registration guard.
    #[must_use]
    pub fn into_parts(self) -> (Child, RegistrationGuard) {
        let mut this = ManuallyDrop::new(self);
        let guard = this
            .guard
            .take()
            .unwrap_or_else(|| ProcessSupervisor::new(this.lifecycle).register_pid(0));
        // SAFETY: `this` is wrapped in `ManuallyDrop`, and `child` is read exactly once to transfer ownership to the caller. The wrapper destructor will not run afterward.
        let child = unsafe { std::ptr::read(&this.child) };
        (child, guard)
    }
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

impl SupervisedAsyncChild {
    /// Split the wrapper into the child handle and registration guard.
    #[must_use]
    pub fn into_parts(self) -> (TokioChild, RegistrationGuard) {
        let mut this = ManuallyDrop::new(self);
        let guard = this
            .guard
            .take()
            .unwrap_or_else(|| ProcessSupervisor::new(this.lifecycle).register_pid(0));
        // SAFETY: `this` is wrapped in `ManuallyDrop`, and `child` is read exactly once to transfer ownership to the caller. The wrapper destructor will not run afterward.
        let child = unsafe { std::ptr::read(&this.child) };
        (child, guard)
    }
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
