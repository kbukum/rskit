#![allow(missing_docs)]
#![cfg(unix)]

//! A group whose leader exits cleanly but whose descendants live on is retained.
//!
//! When a group target's leader exits `0` after backgrounding a descendant, the
//! run has "completed" from the leader's point of view, but the isolated process
//! group is still alive. Inferring group liveness from the leader alone would
//! unregister the target on leader-reap and leak the descendants. These tests
//! assert the run instead retains the still-live group under an injected
//! supervisor, so its shutdown backstop reaps the whole subtree.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use rskit_process::{
    CapturedIo, LifecyclePolicy, ProcessConfig, ProcessIo, ProcessSpec, ProcessSupervisor,
    run_supervised, run_with_cancel_supervised,
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

async fn read_pid_async(path: &Path) -> u32 {
    for _ in 0..500 {
        if let Ok(text) = std::fs::read_to_string(path)
            && let Ok(pid) = text.trim().parse::<u32>()
        {
            return pid;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("pid file never became a valid pid: {}", path.display());
}

async fn wait_until_gone(pid: u32) -> bool {
    for _ in 0..250 {
        if !pid_exists(pid) {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    !pid_exists(pid)
}

/// A group leader that backgrounds a descendant and then exits `0` without
/// `wait`ing for it, so the leader is reaped while the group lives on.
fn leader_exits_before_group(pid_file: &Path) -> ProcessSpec {
    ProcessSpec::new("/bin/sh").args([
        "-c".to_string(),
        format!(
            "sleep 30 >/dev/null 2>&1 & printf %s \"$!\" > '{}'; exit 0",
            pid_file.display()
        ),
    ])
}

fn short_grace() -> LifecyclePolicy {
    LifecyclePolicy::default().with_grace_period(Duration::from_millis(100))
}

#[tokio::test]
async fn async_run_retains_a_group_that_outlives_its_leader() {
    let workspace = TestWorkspace::new("survivor-async");
    let pid_file = workspace.child("gc.pid").expect("pid path");
    let supervisor = Arc::new(ProcessSupervisor::new(short_grace()));

    let spec = leader_exits_before_group(&pid_file);
    let config = ProcessConfig::default()
        .with_timeout(None)
        .with_io(ProcessIo::captured(CapturedIo::new()))
        .with_lifecycle_policy(short_grace());

    let result = run_with_cancel_supervised(&supervisor, &spec, &config, CancellationToken::new())
        .await
        .expect("run completes");
    assert_eq!(result.exit_code, Some(0), "leader exited cleanly");

    let grandchild = read_pid_async(&pid_file).await;
    assert!(
        pid_exists(grandchild),
        "the backgrounded descendant must still be alive after the leader exits"
    );
    assert_eq!(
        supervisor.registry_len(),
        1,
        "a group outliving its leader must be retained, not unregistered on leader-reap"
    );

    supervisor
        .shutdown("test backstop")
        .await
        .expect("shutdown");
    assert!(
        wait_until_gone(grandchild).await,
        "the shared supervisor must reap the retained group on shutdown"
    );
    assert_eq!(supervisor.registry_len(), 0);
}

#[tokio::test]
async fn sync_run_retains_a_group_that_outlives_its_leader() {
    let workspace = TestWorkspace::new("survivor-sync");
    let pid_file = workspace.child("gc.pid").expect("pid path");
    let supervisor = Arc::new(ProcessSupervisor::new(short_grace()));

    let spec = leader_exits_before_group(&pid_file);
    let config = ProcessConfig::default()
        .with_timeout(None)
        .with_io(ProcessIo::captured(CapturedIo::new()))
        .with_lifecycle_policy(short_grace());

    let run_supervisor = Arc::clone(&supervisor);
    let result = tokio::task::spawn_blocking(move || {
        run_supervised(&run_supervisor, &spec, &config).expect("run completes")
    })
    .await
    .expect("blocking run joins");
    assert_eq!(result.exit_code, Some(0), "leader exited cleanly");

    let grandchild = read_pid_async(&pid_file).await;
    assert!(
        pid_exists(grandchild),
        "the backgrounded descendant must still be alive after the leader exits"
    );
    assert_eq!(
        supervisor.registry_len(),
        1,
        "a group outliving its leader must be retained, not unregistered on leader-reap"
    );

    supervisor
        .shutdown("test backstop")
        .await
        .expect("shutdown");
    assert!(
        wait_until_gone(grandchild).await,
        "the shared supervisor must reap the retained group on shutdown"
    );
    assert_eq!(supervisor.registry_len(), 0);
}
