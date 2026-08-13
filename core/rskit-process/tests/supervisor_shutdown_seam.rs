#![allow(missing_docs)]
#![cfg(unix)]

use std::time::{Duration, Instant};

use rskit_process::{LifecyclePolicy, ProcessSupervisor};
use rskit_testutil::TestWorkspace;
use tokio::process::Command;
use tokio_util::sync::CancellationToken;

fn pid_exists(pid: u32) -> bool {
    let Ok(pid) = i32::try_from(pid) else {
        return false;
    };
    // SAFETY: signal 0 performs an existence check without delivering a signal.
    unsafe { libc::kill(pid, 0) == 0 }
}

/// Spawn a supervised group whose reparentable grandchild pid is written to `pid_file`.
///
/// The direct child becomes a zombie of the test process once killed, so behavioral
/// assertions target the grandchild `sleep`, which init reaps to `ESRCH` after a group kill.
fn spawn_group(
    supervisor: &ProcessSupervisor,
    pid_file: &std::path::Path,
) -> rskit_process::SupervisedAsyncChild {
    let mut command = Command::new("/bin/sh");
    command
        .args([
            "-c",
            &format!(
                "sleep 30 & printf %s \"$!\" > '{}'; wait",
                pid_file.display()
            ),
        ])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    supervisor.spawn_async(&mut command).expect("child spawns")
}

fn read_pid(pid_file: &std::path::Path) -> u32 {
    std::fs::read_to_string(pid_file)
        .expect("pid file")
        .trim()
        .parse()
        .expect("numeric pid")
}

async fn wait_for_files(files: &[std::path::PathBuf]) {
    for _ in 0..500 {
        if files
            .iter()
            .all(|file| file.metadata().map(|meta| meta.len() > 0).unwrap_or(false))
        {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("child pid files were not written in time");
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

#[tokio::test]
async fn tripping_shutdown_handle_reaps_all_registered_groups_without_run_future() {
    let workspace = TestWorkspace::new("seam-trip-reaps-all");
    let supervisor = ProcessSupervisor::new(
        LifecyclePolicy::default().with_grace_period(Duration::from_millis(100)),
    );

    let mut children = Vec::new();
    let mut pid_files = Vec::new();
    for index in 0..3 {
        let pid_file = workspace
            .child(format!("gc-{index}.pid"))
            .expect("pid path");
        children.push(spawn_group(&supervisor, &pid_file));
        pid_files.push(pid_file);
    }
    wait_for_files(&pid_files).await;
    let grandchildren: Vec<u32> = pid_files.iter().map(|file| read_pid(file)).collect();
    assert_eq!(supervisor.registry_len(), 3);

    let token = CancellationToken::new();
    let _subscription = supervisor.subscribe_shutdown(token.clone());
    token.cancel();

    for pid in grandchildren {
        assert!(
            wait_until_gone(pid).await,
            "shutdown backstop must reap every registered group with no run future present"
        );
    }
    drop(children);
}

#[tokio::test]
async fn cooperative_teardown_before_backstop_leaves_a_clean_no_op() {
    let workspace = TestWorkspace::new("seam-cooperative-first");
    let supervisor = ProcessSupervisor::new(
        LifecyclePolicy::default().with_grace_period(Duration::from_millis(100)),
    );
    let pid_file = workspace.child("gc.pid").expect("pid path");
    let child = spawn_group(&supervisor, &pid_file);
    wait_for_files(std::slice::from_ref(&pid_file)).await;
    let grandchild = read_pid(&pid_file);

    // Cooperative teardown reaps and unregisters the group ahead of any backstop.
    drop(child);
    assert!(
        wait_until_gone(grandchild).await,
        "cooperative teardown should reap the group"
    );
    assert_eq!(supervisor.registry_len(), 0);

    // The backstop over an already-reaped registry is a clean no-op: no error, no double-kill.
    let token = CancellationToken::new();
    let _subscription = supervisor.subscribe_shutdown(token.clone());
    token.cancel();
    supervisor
        .shutdown("post-cooperative backstop")
        .await
        .expect("backstop over reaped children is a clean no-op");
    assert_eq!(supervisor.registry_len(), 0);
    assert!(!pid_exists(grandchild));
}

#[tokio::test]
async fn concurrent_cancel_and_fan_out_complete_within_a_bounded_budget() {
    let workspace = TestWorkspace::new("seam-no-deadlock");
    let supervisor = ProcessSupervisor::new(
        LifecyclePolicy::default().with_grace_period(Duration::from_millis(50)),
    );

    let mut children = Vec::new();
    let mut pid_files = Vec::new();
    for index in 0..4 {
        let pid_file = workspace
            .child(format!("gc-{index}.pid"))
            .expect("pid path");
        children.push(spawn_group(&supervisor, &pid_file));
        pid_files.push(pid_file);
    }
    wait_for_files(&pid_files).await;

    let token = CancellationToken::new();
    let _subscription = supervisor.subscribe_shutdown(token.clone());

    let started = Instant::now();
    // First-signal cancel drives the backstop; both must settle inside the budget.
    token.cancel();
    let drained = tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if supervisor.registry_len() == 0 {
                return;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
    })
    .await;

    assert!(drained.is_ok(), "cancel + fan-out must not deadlock");
    assert!(started.elapsed() < Duration::from_secs(3));
    drop(children);
}
