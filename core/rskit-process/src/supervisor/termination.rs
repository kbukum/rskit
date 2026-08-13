//! Unified SIGTERM → grace → SIGKILL escalation and reaping.

use std::process::{Child, ExitStatus};
use std::thread;
use std::time::{Duration, Instant};

use tokio::{process::Child as TokioChild, time::timeout};
use tracing::{debug, warn};

use crate::command::LifecyclePolicy;
use crate::process_group::{kill_target, signal_target, target_alive, terminate_target};
use crate::signal::ProcessSignal;
use crate::{AppError, AppResult};

const POLL_INTERVAL: Duration = Duration::from_millis(10);

pub(crate) fn reap_within(
    child: &mut Child,
    policy: LifecyclePolicy,
    grace: Duration,
) -> AppResult<(ExitStatus, bool)> {
    let deadline = Instant::now() + grace;
    loop {
        if let Some(status) = child.try_wait().map_err(AppError::internal)? {
            return Ok((status, false));
        }
        if Instant::now() >= deadline {
            if policy.kill_after_grace && !kill_target(child.id(), policy.targets_group()) {
                child.kill().map_err(AppError::internal)?;
            }
            let status = child.wait().map_err(AppError::internal)?;
            return Ok((status, policy.kill_after_grace));
        }
        thread::sleep(POLL_INTERVAL);
    }
}

pub(crate) fn terminate_and_reap(
    child: &mut Child,
    policy: LifecyclePolicy,
    grace: Duration,
) -> AppResult<(ExitStatus, bool)> {
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
) -> (Option<i32>, Option<String>) {
    if !pid.is_some_and(|pid| terminate_target(pid, policy.targets_group())) {
        let _ = child.start_kill();
    }
    match timeout(policy.grace_period, child.wait()).await {
        Ok(Ok(status)) => (status.code(), None),
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
            escalate_after_grace(child, pid, policy, &format!("grace period expired after {reason}"))
                .await
        }
    }
}

/// Force-kill escalation after the graceful grace period.
///
/// When escalation is enabled this force-kills, reaps, and reports the `SIGKILL`. When
/// [`LifecyclePolicy::kill_after_grace`] is disabled it must not force-kill and must not
/// block waiting on a process that may ignore `SIGTERM`, so it reports the unmet grace
/// period truthfully rather than claiming a kill that never happened.
async fn escalate_after_grace(
    child: &mut TokioChild,
    pid: Option<u32>,
    policy: LifecyclePolicy,
    context: &str,
) -> (Option<i32>, Option<String>) {
    if !policy.kill_after_grace {
        return (
            None,
            Some(format!("{context}; kill escalation disabled")),
        );
    }
    if !pid.is_some_and(|pid| kill_target(pid, policy.targets_group())) {
        let _ = child.start_kill();
    }
    let _ = child.wait().await;
    (None, Some(format!("process killed by SIGKILL ({context})")))
}

/// Terminate a registered target and report whether it can be unregistered.
///
/// Sends graceful termination, waits the grace period, and — when escalation is enabled —
/// force-kills, returning `true` in that case because a best-effort kill was issued. When
/// escalation is disabled the target may deliberately survive `SIGTERM`, so this returns
/// `true` only once the target is confirmed gone; otherwise the caller keeps it registered
/// so a later shutdown or supervisor drop can retry.
pub(crate) async fn terminate_registered_pid(
    pid: u32,
    process_group: bool,
    policy: LifecyclePolicy,
) -> bool {
    if pid == 0 {
        return true;
    }
    let _ = signal_target(pid, ProcessSignal::Terminate, process_group);
    tokio::time::sleep(policy.grace_period).await;
    if policy.kill_after_grace {
        let _ = signal_target(pid, ProcessSignal::Kill, process_group);
        return true;
    }
    !target_alive(pid)
}
