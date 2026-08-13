use std::num::NonZeroI32;
use std::time::Duration;

use rskit_errors::{AppError, AppResult, ErrorCode};
use tokio::runtime::Handle;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;

use super::ShutdownPolicy;

/// Installed shutdown signal controller and cooperative cancellation handle.
#[derive(Debug)]
pub struct ShutdownController {
    token: CancellationToken,
    supervisor: JoinHandle<()>,
}

impl ShutdownController {
    /// Install the configured shutdown signal handlers inside the current Tokio runtime.
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
