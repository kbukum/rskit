//! Shared blocking termination for the std-thread runners (`sync` and
//! `persistent`).
//!
//! Both blocking runners terminate a child the same way — deliver a graceful
//! signal, wait a grace period, then escalate to `SIGKILL` — and both target the
//! process group when the signal policy asks for descendant termination. This
//! module owns that escalation once, routing every signal through
//! [`crate::process_group`] so there is a single place that talks to the OS.

use std::process::{Child, ExitStatus};
use std::thread;
use std::time::{Duration, Instant};

use crate::process_group::{kill_target, terminate_target};
use crate::{AppError, AppResult, SignalPolicy};

const POLL_INTERVAL: Duration = Duration::from_millis(10);

/// Whether termination should target the whole process group.
pub(crate) const fn targets_group(signal: SignalPolicy) -> bool {
    signal.create_process_group && signal.terminate_descendants
}

/// Wait up to `grace` for `child` to exit, escalating to `SIGKILL` when it does
/// not.
///
/// Assumes a graceful signal was already delivered. Returns the exit status and
/// whether escalation to `SIGKILL` was required.
pub(crate) fn reap_within(
    child: &mut Child,
    pid: u32,
    signal: SignalPolicy,
    grace: Duration,
) -> AppResult<(ExitStatus, bool)> {
    let deadline = Instant::now() + grace;
    loop {
        if let Some(status) = child.try_wait().map_err(AppError::internal)? {
            return Ok((status, false));
        }
        if Instant::now() >= deadline {
            if !kill_target(pid, targets_group(signal)) {
                child.kill().map_err(AppError::internal)?;
            }
            let status = child.wait().map_err(AppError::internal)?;
            return Ok((status, true));
        }
        thread::sleep(POLL_INTERVAL);
    }
}

/// Deliver a graceful termination signal, then reap within `grace`, escalating
/// to `SIGKILL` if the child outlives the grace period.
///
/// Returns the exit status and whether escalation to `SIGKILL` was required.
pub(crate) fn terminate_and_reap(
    child: &mut Child,
    pid: u32,
    signal: SignalPolicy,
    grace: Duration,
) -> AppResult<(ExitStatus, bool)> {
    if !terminate_target(pid, targets_group(signal)) {
        child.kill().map_err(AppError::internal)?;
    }
    reap_within(child, pid, signal, grace)
}

#[cfg(all(test, unix))]
mod tests {
    use std::process::{Command, Stdio};
    use std::time::Duration;

    use super::*;

    fn spawn_loop() -> Child {
        Command::new("/bin/sh")
            .arg("-c")
            .arg("while :; do sleep 1; done")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("child starts")
    }

    #[test]
    fn terminate_and_reap_stops_a_running_child() {
        let mut child = spawn_loop();
        let pid = child.id();
        let policy = SignalPolicy::default()
            .with_create_process_group(false)
            .with_terminate_descendants(false);

        let (status, _escalated) =
            terminate_and_reap(&mut child, pid, policy, Duration::from_millis(200))
                .expect("child is reaped");
        assert!(!status.success());
    }

    #[test]
    fn reap_within_escalates_when_graceful_signal_is_ignored() {
        use std::io::Read;

        // Ignore SIGTERM so the grace period expires and SIGKILL is required.
        // The child prints a marker only after the trap is installed, so the
        // test cannot race SIGTERM ahead of the trap.
        let mut child = Command::new("/bin/sh")
            .arg("-c")
            .arg("trap '' TERM; echo ready; while :; do sleep 1; done")
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("child starts");
        let pid = child.id();

        let mut marker = [0_u8; 5];
        child
            .stdout
            .as_mut()
            .expect("piped stdout")
            .read_exact(&mut marker)
            .expect("trap installed");
        assert_eq!(&marker, b"ready");

        let policy = SignalPolicy::default()
            .with_create_process_group(false)
            .with_terminate_descendants(false);

        assert!(terminate_target(pid, false));
        let (_status, escalated) = reap_within(&mut child, pid, policy, Duration::from_millis(50))
            .expect("child is reaped");
        assert!(escalated, "an ignored SIGTERM must escalate to SIGKILL");
    }
}
