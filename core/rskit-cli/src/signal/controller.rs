use std::num::NonZeroI32;
use std::time::Duration;

use rskit_errors::{AppError, AppResult, ErrorCode};
use tokio::runtime::Handle;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use super::ShutdownPolicy;

/// Installed shutdown signal controller and cooperative cancellation handle.
///
/// Installing a controller registers process-wide signal handlers through Tokio.
/// On Unix that registration is permanent for the life of the process: Tokio
/// does not restore a signal's default disposition when the streams are dropped.
/// Dropping the controller therefore aborts the background task that watches for
/// signals and cancels the token, leaving `SIGINT`/`SIGTERM`/`SIGHUP` captured
/// with nothing left to act on them. Hold the controller for as long as
/// coordinated shutdown is required — typically the whole lifetime of the
/// process — rather than letting it drop at the end of a scope.
#[derive(Debug)]
#[must_use = "dropping the ShutdownController aborts its signal handler task and leaves the \
              installed signals captured with no task to cancel the token or force-exit; hold it \
              for the lifetime of the process"]
pub struct ShutdownController {
    token: CancellationToken,
    supervisor: JoinHandle<()>,
}

impl ShutdownController {
    /// Install the configured shutdown signal handlers inside the current Tokio runtime.
    ///
    /// The returned controller must be retained: its background task owns the
    /// only receiver for the installed signals and cancels the token (or
    /// force-exits on a second signal). Because the underlying registration is
    /// process-wide and permanent on Unix, discarding the controller does not
    /// restore default signal handling — it only abandons the coordinated
    /// shutdown behavior.
    pub fn install(policy: ShutdownPolicy) -> AppResult<Self> {
        let handle = Handle::try_current().map_err(|err| {
            AppError::new(
                ErrorCode::Internal,
                "install shutdown controller: no active Tokio runtime",
            )
            .with_cause(err)
        })?;
        let token = CancellationToken::new();
        let supervisor_token = token.clone();
        let supervisor = install_platform(policy, supervisor_token, &handle)?;

        Ok(Self { token, supervisor })
    }

    /// Return a clone of the cooperative cancellation token.
    #[must_use]
    pub fn token(&self) -> CancellationToken {
        self.token.clone()
    }
}

impl Drop for ShutdownController {
    fn drop(&mut self) {
        self.supervisor.abort();
    }
}

#[cfg(unix)]
fn install_platform(
    policy: ShutdownPolicy,
    token: CancellationToken,
    handle: &Handle,
) -> AppResult<JoinHandle<()>> {
    let ShutdownPolicy {
        signals,
        drain_deadline,
        second_signal_exit_code: exit_code,
    } = policy;
    let mut streams = super::unix::SignalStreams::install(&signals)?;

    Ok(handle.spawn(async move {
        streams.recv().await;
        token.cancel();
        await_escalation(exit_code, drain_deadline, async move {
            streams.recv().await;
        })
        .await;
    }))
}

#[cfg(windows)]
fn install_platform(
    policy: ShutdownPolicy,
    token: CancellationToken,
    handle: &Handle,
) -> AppResult<JoinHandle<()>> {
    let ShutdownPolicy {
        signals,
        drain_deadline,
        second_signal_exit_code: exit_code,
    } = policy;
    let mut streams = super::windows::SignalStreams::install(&signals)?;

    Ok(handle.spawn(async move {
        streams.recv().await;
        token.cancel();
        await_escalation(exit_code, drain_deadline, async move {
            streams.recv().await;
        })
        .await;
    }))
}

async fn await_escalation(
    exit_code: NonZeroI32,
    drain_deadline: Option<Duration>,
    next_signal: impl std::future::Future<Output = ()>,
) {
    if let Some(deadline) = drain_deadline {
        tokio::select! {
            () = next_signal => force_exit(exit_code),
            () = tokio::time::sleep(deadline) => force_exit(exit_code),
        }
    } else {
        next_signal.await;
        force_exit(exit_code);
    }
}

#[allow(clippy::disallowed_methods)]
fn force_exit(exit_code: NonZeroI32) -> ! {
    std::process::exit(exit_code.get());
}

/// Install a first-Ctrl+C handler and return the cooperative cancellation token.
///
/// A convenience wrapper over [`ShutdownController`] for the common single-signal case. Must be
/// called from within a Tokio runtime: a background task awaits the interrupt signal and cancels
/// the returned token on the first Ctrl+C. Clone the token and hand it to spawned tasks, an
/// `rskit-worker` handler, or an `rskit-process` call; holders observe `is_cancelled()` /
/// `cancelled()` and wind down gracefully. For drain deadlines, second-signal force-exit, or a
/// custom signal set, install a [`ShutdownController`] directly.
#[must_use]
pub fn on_ctrl_c() -> CancellationToken {
    let token = CancellationToken::new();
    let child = token.clone();
    tokio::spawn(async move {
        if tokio::signal::ctrl_c().await.is_ok() {
            child.cancel();
        }
    });
    token
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn on_ctrl_c_returns_a_live_uncancelled_token() {
        let token = on_ctrl_c();
        assert!(!token.is_cancelled());
        // Local cancellation still works; the interrupt handler is just one source.
        token.cancel();
        token.cancelled().await;
        assert!(token.is_cancelled());
    }
}
