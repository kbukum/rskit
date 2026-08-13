//! Owner-side SIGTERM → grace → SIGKILL escalation and reaping.
//!
//! These helpers run on the path that *owns* the live child handle (the sync,
//! async, and persistent runners). Because the owner holds the un-reaped child,
//! signalling it by pid targets exactly that child — it cannot have been reaped
//! and its pid cannot have been recycled while the handle is held. Reuse-proof
//! signalling for the *registry* backstop, where no live handle is held, lives on
//! [`OwnedTarget`](super::target::OwnedTarget) instead.

use std::process::{Child, ExitStatus};
use std::thread;
use std::time::{Duration, Instant};

use tokio::{process::Child as TokioChild, time::timeout};
use tracing::{debug, warn};

use crate::command::LifecyclePolicy;
use crate::process_group::{kill_target, terminate_target};
use crate::signal::ProcessSignal;
use crate::{AppError, AppResult};

const POLL_INTERVAL: Duration = Duration::from_millis(10);

/// Outcome of reaping a blocking child within a grace period.
#[derive(Debug)]
pub(crate) enum SyncReap {
    /// The child was reaped; `escalated` records whether a `SIGKILL` was needed.
    Reaped { status: ExitStatus, escalated: bool },
    /// The child ignored `SIGTERM` and escalation is disabled, so it is still
    /// alive and deliberately left running; the owner keeps ownership of it.
    Survived,
}

/// Outcome of terminating an async child after cancellation or timeout.
#[derive(Debug)]
pub(crate) enum AsyncReap {
    /// The child exited; `note` carries any escalation diagnostic.
    Reaped {
        code: Option<i32>,
        note: Option<String>,
    },
    /// The child ignored `SIGTERM` and escalation is disabled, so it is still
    /// alive; `note` records what actually happened and the owner retains it.
    Survived { note: String },
}

pub(crate) fn reap_within(
    child: &mut Child,
    policy: LifecyclePolicy,
    grace: Duration,
) -> AppResult<SyncReap> {
    let deadline = Instant::now() + grace;
    loop {
        if let Some(status) = child.try_wait().map_err(AppError::internal)? {
            return Ok(SyncReap::Reaped {
                status,
                escalated: false,
            });
        }
        if Instant::now() >= deadline {
            if !policy.kill_after_grace {
                // Honour the no-force-kill contract: do not block forever waiting
                // on a child that ignores SIGTERM. Report survival so the owner can
                // keep the child registered for a later shutdown.
                return Ok(SyncReap::Survived);
            }
            if !kill_target(child.id(), policy.targets_group()) {
                child.kill().map_err(AppError::internal)?;
            }
            let status = child.wait().map_err(AppError::internal)?;
            return Ok(SyncReap::Reaped {
                status,
                escalated: true,
            });
        }
        thread::sleep(POLL_INTERVAL);
    }
}

pub(crate) fn terminate_and_reap(
    child: &mut Child,
    policy: LifecyclePolicy,
    grace: Duration,
) -> AppResult<SyncReap> {
    if !terminate_target(child.id(), policy.targets_group()) {
        child.kill().map_err(AppError::internal)?;
    }
    reap_within(child, policy, grace)
}

pub(crate) async fn terminate_and_wait_async(
    child: &mut TokioChild,
    pid: Option<u32>,
    policy: LifecyclePolicy,
    reason: &str,
) -> AsyncReap {
    if !pid.is_some_and(|pid| terminate_target(pid, policy.targets_group())) {
        let _ = child.start_kill();
    }
    match timeout(policy.grace_period, child.wait()).await {
        Ok(Ok(status)) => AsyncReap::Reaped {
            code: status.code(),
            note: None,
        },
        Ok(Err(error)) => {
            warn!(
                signal = ProcessSignal::Terminate.name(),
                "error waiting for process after signal: {error}"
            );
            escalate_after_grace(
                child,
                pid,
                policy,
                &format!("error during grace period after {reason}: {error}"),
            )
            .await
        }
        Err(_) => {
            debug!(
                signal = ProcessSignal::Kill.name(),
                "grace period expired, sending signal"
            );
            escalate_after_grace(
                child,
                pid,
                policy,
                &format!("grace period expired after {reason}"),
            )
            .await
        }
    }
}

/// Force-kill escalation after the graceful grace period.
///
/// When escalation is enabled this force-kills, reaps, and reports the `SIGKILL`.
/// When [`LifecyclePolicy::kill_after_grace`] is disabled it must not force-kill
/// and must not block waiting on a process that may ignore `SIGTERM`, so it
/// reports [`AsyncReap::Survived`] truthfully rather than claiming a kill that
/// never happened; the caller keeps ownership of the still-live child.
async fn escalate_after_grace(
    child: &mut TokioChild,
    pid: Option<u32>,
    policy: LifecyclePolicy,
    context: &str,
) -> AsyncReap {
    if !policy.kill_after_grace {
        return AsyncReap::Survived {
            note: format!("{context}; kill escalation disabled, process left running"),
        };
    }
    if !pid.is_some_and(|pid| kill_target(pid, policy.targets_group())) {
        let _ = child.start_kill();
    }
    let code = match child.wait().await {
        Ok(status) => status.code(),
        Err(_) => None,
    };
    AsyncReap::Reaped {
        code,
        note: Some(format!("process killed by SIGKILL ({context})")),
    }
}
