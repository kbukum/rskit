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
use crate::process_group::{group_alive, kill_target, terminate_target};
use crate::signal::ProcessSignal;
use crate::{AppError, AppResult};

const POLL_INTERVAL: Duration = Duration::from_millis(10);

/// Block until the process group `pgid` is empty or the budget expires.
///
/// Descendants are not this process's children, so they cannot be `wait`ed on;
/// after a group `SIGKILL` this bounded poll confirms the subtree actually
/// drained instead of reporting a kill that a lingering descendant outlived.
fn wait_group_gone_blocking(pgid: u32, budget: Duration) -> bool {
    let deadline = Instant::now() + budget;
    loop {
        if !group_alive(pgid) {
            return true;
        }
        if Instant::now() >= deadline {
            return false;
        }
        thread::sleep(POLL_INTERVAL);
    }
}

/// Await until the process group `pgid` is empty or the budget expires.
async fn wait_group_gone_async(pgid: u32, budget: Duration) -> bool {
    let deadline = tokio::time::Instant::now() + budget;
    loop {
        if !group_alive(pgid) {
            return true;
        }
        let now = tokio::time::Instant::now();
        if now >= deadline {
            return false;
        }
        tokio::time::sleep(Duration::from_millis(10).min(deadline.saturating_duration_since(now)))
            .await;
    }
}

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
    let group = policy.targets_group();
    let pgid = child.id();
    let deadline = Instant::now() + grace;
    let mut leader_status: Option<ExitStatus> = None;
    loop {
        if leader_status.is_none() {
            leader_status = child.try_wait().map_err(AppError::internal)?;
        }
        if let Some(status) = leader_status {
            // The leader is reaped. A group target owns a subtree, so it is only
            // done once the whole process group is empty; a leader-only view would
            // let a surviving descendant leak here.
            if !group || !group_alive(pgid) {
                return Ok(SyncReap::Reaped {
                    status,
                    escalated: false,
                });
            }
        }
        if Instant::now() >= deadline {
            if !policy.kill_after_grace {
                // Honour the no-force-kill contract: do not block forever waiting
                // on a child (or group) that ignores SIGTERM. Report survival so
                // the owner keeps it registered for a later shutdown.
                return Ok(SyncReap::Survived);
            }
            if !kill_target(pgid, group) {
                child.kill().map_err(AppError::internal)?;
            }
            let status = match leader_status {
                Some(status) => status,
                None => child.wait().map_err(AppError::internal)?,
            };
            if group {
                // Confirm the whole subtree drained after the group SIGKILL, not
                // just the leader.
                wait_group_gone_blocking(pgid, grace);
            }
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
        // Graceful signalling is unavailable (for example on Windows, where
        // `terminate_target` always fails). Reap a child that already exited on
        // its own, but only fall back to a hard kill when escalation is enabled;
        // otherwise honour `kill_after_grace(false)` and leave the running child
        // alive rather than force-killing it here.
        if let Some(status) = child.try_wait().map_err(AppError::internal)? {
            return Ok(SyncReap::Reaped {
                status,
                escalated: false,
            });
        }
        if !policy.kill_after_grace {
            return Ok(SyncReap::Survived);
        }
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
        // Graceful signalling is unavailable. Only fall back to a hard kill when
        // escalation is enabled; otherwise leave the child for the grace-period
        // wait below, which reaps it if it exits and reports `Survived` under
        // `kill_after_grace(false)` instead of force-killing it.
        if policy.kill_after_grace {
            let _ = child.start_kill();
        }
    }
    match timeout(policy.grace_period, child.wait()).await {
        Ok(Ok(status)) => {
            // The leader exited within the grace period. A group target owns a
            // subtree, so confirm the whole process group drained; if a
            // descendant outlived the leader, escalate against the surviving
            // group (or report it survived under `kill_after_grace = false`).
            if policy.targets_group() && group_alive(pid.unwrap_or_default()) {
                escalate_after_grace(
                    child,
                    pid,
                    policy,
                    &format!("group outlived leader after {reason}"),
                )
                .await
            } else {
                AsyncReap::Reaped {
                    code: status.code(),
                    note: None,
                }
            }
        }
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
/// For a group target it also waits (bounded) for the whole process group to
/// drain, so a `SIGKILL`-escalated subtree is confirmed gone rather than assumed
/// dead once the leader is reaped. When [`LifecyclePolicy::kill_after_grace`] is
/// disabled it must not force-kill and must not block waiting on a process (or
/// group) that may ignore `SIGTERM`, so it reports [`AsyncReap::Survived`]
/// truthfully rather than claiming a kill that never happened; the caller keeps
/// ownership of the still-live child (and its group).
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
    let group = policy.targets_group();
    if !pid.is_some_and(|pid| kill_target(pid, group)) {
        let _ = child.start_kill();
    }
    let code = match child.wait().await {
        Ok(status) => status.code(),
        Err(_) => None,
    };
    if group {
        // Confirm the whole subtree drained after the group SIGKILL, not just the
        // leader.
        wait_group_gone_async(pid.unwrap_or_default(), policy.grace_period).await;
    }
    AsyncReap::Reaped {
        code,
        note: Some(format!("process killed by SIGKILL ({context})")),
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::process_group::{group_alive, isolate, isolate_async};
    use std::process::{Command, Stdio};
    use tokio::io::AsyncReadExt;

    /// The stubborn-group script: a process-group leader backgrounds a
    /// `SIGTERM`-ignoring descendant (with its own `/dev/null` stdio, so it never
    /// holds the leader's pipe open), waits race-free for the descendant to arm
    /// its `trap`, prints `ready`, and exits cleanly — so the group outlives its
    /// leader.
    const STUBBORN_GROUP: &str = "F=$(mktemp); \
         (trap '' TERM; echo 1 > \"$F\"; while :; do sleep 30; done) >/dev/null 2>&1 & \
         until [ -s \"$F\" ]; do :; done; rm -f \"$F\"; printf ready; exit 0";

    fn read_ready_blocking(child: &mut Child) {
        use std::io::Read;
        let mut ready = [0_u8; 5];
        child
            .stdout
            .take()
            .expect("stdout")
            .read_exact(&mut ready)
            .expect("read readiness");
        assert_eq!(&ready, b"ready");
    }

    async fn read_ready_async(child: &mut TokioChild) {
        let mut ready = [0_u8; 5];
        child
            .stdout
            .take()
            .expect("stdout")
            .read_exact(&mut ready)
            .await
            .expect("read readiness");
        assert_eq!(&ready, b"ready");
    }

    fn group_policy() -> LifecyclePolicy {
        LifecyclePolicy::default().with_grace_period(Duration::from_secs(2))
    }

    /// `reap_within` must not report the target reaped while the isolated group
    /// outlives its cleanly-exited leader; with escalation it force-kills the
    /// surviving group and confirms it drained.
    #[test]
    fn reap_within_escalates_to_a_group_that_outlives_its_leader() {
        let mut command = Command::new("/bin/sh");
        command
            .args(["-c", STUBBORN_GROUP])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        isolate(&mut command);
        let mut child = command.spawn().expect("group-leader child spawns");
        let pgid = child.id();
        read_ready_blocking(&mut child);

        let reap =
            reap_within(&mut child, group_policy(), Duration::from_secs(2)).expect("reap succeeds");
        match reap {
            SyncReap::Reaped { escalated, .. } => assert!(
                escalated,
                "a group outliving its leader must force escalation"
            ),
            SyncReap::Survived => panic!("group must be reaped under kill_after_grace"),
        }
        assert!(
            !group_alive(pgid),
            "the group must be gone after escalation"
        );
    }

    /// With `kill_after_grace = false`, `reap_within` must report the survivor
    /// truthfully and leave the group running rather than force-killing it.
    #[test]
    fn reap_within_reports_survival_for_a_relinquished_group() {
        let mut command = Command::new("/bin/sh");
        command
            .args(["-c", STUBBORN_GROUP])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        isolate(&mut command);
        let mut child = command.spawn().expect("group-leader child spawns");
        let pgid = child.id();
        read_ready_blocking(&mut child);

        let policy = group_policy().with_kill_after_grace(false);
        let reap =
            reap_within(&mut child, policy, Duration::from_millis(200)).expect("reap succeeds");
        assert!(
            matches!(reap, SyncReap::Survived),
            "a live group with escalation disabled must report Survived"
        );
        assert!(group_alive(pgid), "the group must still be running");
        kill_target(pgid, true);
    }

    /// The async escalation path force-kills a group that outlives its leader and
    /// confirms the subtree drained.
    #[tokio::test]
    async fn terminate_and_wait_async_escalates_to_a_surviving_group() {
        let mut command = tokio::process::Command::new("/bin/sh");
        command
            .args(["-c", STUBBORN_GROUP])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null());
        isolate_async(&mut command);
        let mut child = command.spawn().expect("group-leader child spawns");
        let pgid = child.id().expect("live pid");
        read_ready_async(&mut child).await;

        let reap = terminate_and_wait_async(&mut child, Some(pgid), group_policy(), "test").await;
        assert!(
            matches!(reap, AsyncReap::Reaped { .. }),
            "a group outliving its leader must be reaped under kill_after_grace"
        );
        assert!(
            !group_alive(pgid),
            "the group must be gone after escalation"
        );
    }
}
