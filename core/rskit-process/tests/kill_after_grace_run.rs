#![allow(missing_docs)]
#![cfg(unix)]

//! `kill_after_grace(false)` keeps honest ownership of a `SIGTERM`-ignoring
//! survivor across the run entry points.
//!
//! When a timed-out child ignores `SIGTERM` and escalation is disabled, the run
//! must report the timeout honestly (`timed_out`, no exit code, a "left running"
//! note) and *not* force-kill the survivor mid-call. Instead the live child is
//! relinquished to whichever supervisor owns the call, so it is reaped when that
//! supervisor drops rather than leaked as a zombie.
//!
//! - Under an **injected** supervisor the survivor is left running after the run
//!   returns and reaped only when the shared supervisor drops.
//! - Under the **call-scoped** `run`, the per-call supervisor owns the child for
//!   the duration of the call and reaps the survivor when it drops on return —
//!   honest report, no leak, but not left running past the call.

use std::path::Path;
use std::time::Duration;

use rskit_process::{
    CapturedIo, LifecyclePolicy, ProcessConfig, ProcessIo, ProcessSpec, ProcessSupervisor, run,
    run_with_cancel_supervised,
};
use rskit_testutil::TestWorkspace;
use tokio_util::sync::CancellationToken;

fn pid_exists(pid: u32) -> bool {
    let Ok(pid) = i32::try_from(pid) else {
        return false;
    };
    // SAFETY: signal 0 performs an existence check without delivering a signal.
    unsafe { libc::kill(pid, 0) == 0 }
}

fn read_pid_blocking(path: &Path) -> u32 {
    for _ in 0..500 {
        if let Ok(text) = std::fs::read_to_string(path)
            && let Ok(pid) = text.trim().parse::<u32>()
        {
            return pid;
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    panic!("pid file never became a valid pid: {}", path.display());
}

async fn wait_until_gone_async(pid: u32) -> bool {
    for _ in 0..250 {
        if !pid_exists(pid) {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    !pid_exists(pid)
}

fn wait_until_gone_blocking(pid: u32) -> bool {
    for _ in 0..250 {
        if !pid_exists(pid) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    !pid_exists(pid)
}

/// A child that records its own pid and then ignores `SIGTERM` forever, so a
/// grace-limited terminate can only reap it by escalating to `SIGKILL`.
fn stubborn_sigterm_ignorer(pid_file: &Path) -> ProcessSpec {
    ProcessSpec::new("/bin/sh").args([
        "-c".to_string(),
        format!(
            "trap '' TERM; printf %s \"$$\" > '{}'; while :; do sleep 1; done",
            pid_file.display()
        ),
    ])
}

fn no_force_kill_short_grace() -> LifecyclePolicy {
    LifecyclePolicy::default()
        .with_grace_period(Duration::from_millis(100))
        .with_kill_after_grace(false)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn injected_run_leaves_kill_after_grace_false_survivor_running_until_supervisor_drop() {
    let workspace = TestWorkspace::new("kag-false-injected");
    let pid_file = workspace.child("leader.pid").expect("pid path");
    let supervisor = ProcessSupervisor::new(no_force_kill_short_grace());

    let spec = stubborn_sigterm_ignorer(&pid_file);
    let config = ProcessConfig::default()
        .with_timeout(Some(Duration::from_millis(50)))
        .with_io(ProcessIo::captured(CapturedIo::new()))
        .with_lifecycle_policy(no_force_kill_short_grace());

    let result = run_with_cancel_supervised(&supervisor, &spec, &config, CancellationToken::new())
        .await
        .expect("run completes");

    // The run reports the timeout honestly instead of pretending the process died.
    assert!(result.timed_out, "the run timed out");
    assert_eq!(
        result.exit_code, None,
        "a survivor has no exit code, not a success-shaped zero"
    );
    assert!(
        result.stderr.contains("kill escalation disabled"),
        "the survivor note must record that escalation was disabled: {:?}",
        result.stderr
    );

    // The child ignored SIGTERM and was not force-killed mid-call: it is left
    // running, relinquished to the injected supervisor.
    let leader = read_pid_blocking(&pid_file);
    assert!(
        pid_exists(leader),
        "kill_after_grace(false) must leave the SIGTERM-ignoring survivor running"
    );
    assert_eq!(
        supervisor.registry_len(),
        1,
        "the survivor stays registered with the injected supervisor"
    );

    // Dropping the shared supervisor is the final backstop: it reaps the survivor,
    // so the honest "left running" report never means a leaked zombie.
    drop(supervisor);
    assert!(
        wait_until_gone_async(leader).await,
        "dropping the injected supervisor must reap the relinquished survivor"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn call_scoped_run_reaps_kill_after_grace_false_survivor_on_return_without_leaking() {
    let workspace = TestWorkspace::new("kag-false-call-scoped");
    let pid_file = workspace.child("leader.pid").expect("pid path");

    let spec = stubborn_sigterm_ignorer(&pid_file);
    let config = ProcessConfig::default()
        .with_timeout(Some(Duration::from_millis(50)))
        .with_io(ProcessIo::captured(CapturedIo::new()))
        .with_lifecycle_policy(no_force_kill_short_grace());

    let pid_file_for_run = pid_file.clone();
    let result = tokio::task::spawn_blocking(move || {
        let result = run(&spec, &config).expect("run completes");
        // Capture the survivor pid before the throwaway supervisor drops on return.
        let leader = read_pid_blocking(&pid_file_for_run);
        (result, leader)
    })
    .await
    .expect("blocking run joins");
    let (result, leader) = result;

    // The call-scoped run reports the timeout honestly, exactly like the injected case.
    assert!(result.timed_out, "the run timed out");
    assert_eq!(result.exit_code, None, "a survivor has no exit code");
    assert!(
        result.stderr.contains("kill escalation disabled"),
        "the survivor note must record that escalation was disabled: {:?}",
        result.stderr
    );

    // But the per-call supervisor owns the child only for the call: it reaps the
    // survivor when it drops on return, so a call-scoped spawn does not leak even
    // though escalation was disabled.
    assert!(
        wait_until_gone_blocking(leader),
        "the per-call supervisor must reap the survivor when it drops on return"
    );
}
