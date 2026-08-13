#![allow(missing_docs)]
#![cfg(unix)]

//! An injected supervisor tracks children spawned by the run entry points.
//!
//! The plain `run_with_cancel` / `run` / `start_persistent_with_cancel` paths
//! each create a throwaway per-call supervisor, so a process-level shutdown
//! cannot reach the children they spawn. The `*_supervised` variants let a
//! caller inject a shared [`ProcessSupervisor`] that registers every spawned
//! child, so the shutdown backstop reaps them even when no run future observes
//! the signal.

use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use rskit_process::{
    CapturedIo, LifecyclePolicy, PersistentConfig, PersistentReadiness, ProcessConfig, ProcessIo,
    ProcessSpec, ProcessSupervisor, run_supervised, run_with_cancel_supervised,
    start_persistent_supervised,
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

async fn wait_for_registration(supervisor: &ProcessSupervisor) {
    for _ in 0..500 {
        if supervisor.registry_len() >= 1 {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("child never registered with the injected supervisor");
}

fn group_sleeper(pid_file: &Path) -> ProcessSpec {
    ProcessSpec::new("/bin/sh").args([
        "-c".to_string(),
        format!(
            "sleep 30 >/dev/null 2>&1 & printf %s \"$!\" > '{}'; wait",
            pid_file.display()
        ),
    ])
}

fn short_grace() -> LifecyclePolicy {
    LifecyclePolicy::default().with_grace_period(Duration::from_millis(100))
}

#[tokio::test]
async fn async_supervised_run_registers_and_shutdown_reaps_the_group() {
    let workspace = TestWorkspace::new("supervised-async");
    let pid_file = workspace.child("gc.pid").expect("pid path");
    let supervisor = Arc::new(ProcessSupervisor::new(short_grace()));

    let spec = group_sleeper(&pid_file);
    let config = ProcessConfig::default()
        .with_timeout(None)
        .with_io(ProcessIo::captured(CapturedIo::new()))
        .with_lifecycle_policy(short_grace());

    let run_supervisor = Arc::clone(&supervisor);
    let run = tokio::spawn(async move {
        let _ =
            run_with_cancel_supervised(&run_supervisor, &spec, &config, CancellationToken::new())
                .await;
    });

    wait_for_registration(&supervisor).await;
    let grandchild = read_pid_async(&pid_file).await;

    // The backstop reaps the whole group even though nothing cancelled the run.
    supervisor
        .shutdown("test backstop")
        .await
        .expect("shutdown");

    assert!(
        wait_until_gone(grandchild).await,
        "supervised async run must let the shared supervisor reap its group"
    );
    let _ = run.await;
    assert_eq!(supervisor.registry_len(), 0);
}

#[tokio::test]
async fn sync_supervised_run_registers_and_shutdown_reaps_the_group() {
    let workspace = TestWorkspace::new("supervised-sync");
    let pid_file = workspace.child("gc.pid").expect("pid path");
    let supervisor = Arc::new(ProcessSupervisor::new(short_grace()));

    let spec = group_sleeper(&pid_file);
    let config = ProcessConfig::default()
        .with_timeout(None)
        .with_io(ProcessIo::captured(CapturedIo::new()))
        .with_lifecycle_policy(short_grace());

    let run_supervisor = Arc::clone(&supervisor);
    let run = tokio::task::spawn_blocking(move || {
        let _ = run_supervised(&run_supervisor, &spec, &config);
    });

    wait_for_registration(&supervisor).await;
    let grandchild = read_pid_async(&pid_file).await;

    supervisor
        .shutdown("test backstop")
        .await
        .expect("shutdown");

    assert!(
        wait_until_gone(grandchild).await,
        "supervised sync run must let the shared supervisor reap its group"
    );
    let _ = run.await;
    assert_eq!(supervisor.registry_len(), 0);
}

#[tokio::test]
async fn persistent_supervised_run_registers_and_shutdown_reaps_the_group() {
    let workspace = TestWorkspace::new("supervised-persistent");
    let pid_file = workspace.child("gc.pid").expect("pid path");
    let supervisor = Arc::new(ProcessSupervisor::new(short_grace()));

    let spec = ProcessSpec::new("/bin/sh").args([
        "-c".to_string(),
        format!(
            "sleep 30 >/dev/null 2>&1 & printf %s \"$!\" > '{}'; printf ready; wait",
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

    let run = start_persistent_supervised(
        &supervisor,
        &spec,
        &process_config,
        &persistent_config,
        CancellationToken::new(),
    )
    .expect("persistent process starts");
    let grandchild = read_pid_async(&pid_file).await;
    assert_eq!(supervisor.registry_len(), 1);

    supervisor
        .shutdown("test backstop")
        .await
        .expect("shutdown");

    assert!(
        wait_until_gone(grandchild).await,
        "supervised persistent run must let the shared supervisor reap its group"
    );
    // Dropping the held process after a backstop reap is a clean no-op.
    drop(run);
}
