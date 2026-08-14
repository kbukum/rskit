#![allow(missing_docs)]
#![cfg(unix)]

//! A group that outlives its leader is reaped even without a shared supervisor.
//!
//! The plain `run` / `run_with_cancel` / `start_persistent_with_cancel` entry
//! points each own a *throwaway* (per-call or per-handle) supervisor rather than
//! an injected one. When a group target's leader exits `0` after backgrounding a
//! descendant, the isolated group is still alive at completion. Inferring group
//! liveness from the reaped leader alone would `disarm` the run and drop the
//! child handle without reaping the group, leaking the descendant.
//!
//! These tests assert the opposite observable outcome: the call retains the
//! still-live group under its own supervisor, so the group is reaped when that
//! supervisor drops as the call (or the persistent handle) goes away — no shared
//! supervisor, no leak.

use std::path::Path;
use std::time::Duration;

use rskit_process::{
    CapturedIo, LifecyclePolicy, PersistentConfig, PersistentReadiness, ProcessConfig, ProcessIo,
    ProcessSpec, run, run_with_cancel, start_persistent_with_cancel,
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

/// A group leader that backgrounds a descendant and then exits `0` without
/// `wait`ing for it, so the leader is reaped while the isolated group lives on.
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
async fn unsupervised_async_run_reaps_a_group_that_outlives_its_leader() {
    let workspace = TestWorkspace::new("unsup-survivor-async");
    let pid_file = workspace.child("gc.pid").expect("pid path");

    let spec = leader_exits_before_group(&pid_file);
    let config = ProcessConfig::default()
        .with_timeout(None)
        .with_io(ProcessIo::captured(CapturedIo::new()))
        .with_lifecycle_policy(short_grace());

    // The descendant pid is written before the leader exits `0`; capture it before
    // the throwaway supervisor tears the group down on return.
    let spec_for_run = spec.clone();
    let config_for_run = config.clone();
    let run_handle = tokio::spawn(async move {
        run_with_cancel(&spec_for_run, &config_for_run, CancellationToken::new())
            .await
            .expect("run completes")
    });
    let grandchild = read_pid_async(&pid_file).await;

    let result = run_handle.await.expect("run task joins");
    assert_eq!(result.exit_code, Some(0), "leader exited cleanly");

    // No injected supervisor exists, so the *per-call* supervisor must have retained
    // and then reaped the surviving group as `run_with_cancel` returned. Inferring
    // liveness from the reaped leader would have leaked the descendant instead.
    assert!(
        wait_until_gone_async(grandchild).await,
        "the per-call supervisor must reap the group that outlived its leader on return"
    );
}

#[tokio::test]
async fn unsupervised_sync_run_reaps_a_group_that_outlives_its_leader() {
    let workspace = TestWorkspace::new("unsup-survivor-sync");
    let pid_file = workspace.child("gc.pid").expect("pid path");

    let spec = leader_exits_before_group(&pid_file);
    let config = ProcessConfig::default()
        .with_timeout(None)
        .with_io(ProcessIo::captured(CapturedIo::new()))
        .with_lifecycle_policy(short_grace());

    let result = tokio::task::spawn_blocking(move || run(&spec, &config).expect("run completes"))
        .await
        .expect("blocking run joins");
    assert_eq!(result.exit_code, Some(0), "leader exited cleanly");

    // The blocking `run` builds a throwaway supervisor that owns the child for the
    // duration of the call; a group outliving the reaped leader is relinquished to
    // it and reaped when it drops on return, rather than being leaked.
    let grandchild = read_pid_blocking(&pid_file);
    assert!(
        wait_until_gone_blocking(grandchild),
        "the per-call supervisor must reap the group that outlived its leader on return"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn persistent_handle_owned_supervisor_reaps_a_group_that_outlives_its_leader() {
    let workspace = TestWorkspace::new("unsup-survivor-persistent");
    let pid_file = workspace.child("gc.pid").expect("pid path");

    // The leader backgrounds a descendant, becomes ready, then exits `0`, so the
    // reaped-leader / live-group split is exercised through the persistent handle.
    let spec = ProcessSpec::new("/bin/sh").args([
        "-c".to_string(),
        format!(
            "sleep 30 >/dev/null 2>&1 & printf %s \"$!\" > '{}'; printf ready; exit 0",
            pid_file.display()
        ),
    ]);
    let process_config = ProcessConfig::default()
        .with_timeout(None)
        .with_io(ProcessIo::captured(CapturedIo::new()))
        .with_lifecycle_policy(short_grace());
    let persistent_config = PersistentConfig::default()
        .with_readiness(PersistentReadiness::OutputContains("ready".to_string()))
        .with_readiness_timeout(Duration::from_secs(5))
        .with_shutdown_grace_period(Duration::from_millis(100));

    // The non-injected entry point makes the returned handle own its own supervisor.
    let run = start_persistent_with_cancel(
        &spec,
        &process_config,
        &persistent_config,
        CancellationToken::new(),
    )
    .expect("persistent process starts");
    let grandchild = read_pid_async(&pid_file).await;
    assert!(
        pid_exists(grandchild),
        "the backgrounded descendant must be alive right after readiness"
    );

    // `wait` reaps the exited leader, detects the surviving group, and relinquishes
    // it to the handle-owned supervisor. Dropping the returned process then drops
    // that supervisor, which must reap the whole surviving group — no injected
    // supervisor is involved.
    let result = tokio::task::spawn_blocking(move || run.process.wait().expect("natural wait"))
        .await
        .expect("wait joins");
    assert_eq!(result.exit_code, Some(0), "leader exited cleanly");

    assert!(
        wait_until_gone_async(grandchild).await,
        "the handle-owned supervisor must reap the group that outlived its leader"
    );
}
