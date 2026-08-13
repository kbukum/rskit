#![allow(missing_docs)]

use std::path::Path;
use std::time::Duration;

use rskit_process::{
    LifecyclePolicy, ProcessConfig, ProcessSpec, ProcessSupervisor, run_with_cancel,
};
use rskit_testutil::TestWorkspace;
use tokio_util::sync::CancellationToken;

#[cfg(unix)]
fn pid_exists(pid: u32) -> bool {
    let Ok(pid) = i32::try_from(pid) else {
        return false;
    };
    // SAFETY: signal 0 performs an existence check without delivering a signal.
    unsafe { libc::kill(pid, 0) == 0 }
}

/// Read a pid a child writes to `path`, tolerating the window between file
/// creation (`>` truncates first) and the pid being written.
#[cfg(unix)]
fn read_pid(path: &Path) -> u32 {
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

/// Async variant of [`read_pid`] that yields to the runtime while waiting, so a
/// child spawned on the same current-thread runtime can make progress.
#[cfg(unix)]
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

#[tokio::test]
#[cfg(unix)]
async fn graceful_signal_reaps_child_and_grandchild_group() {
    let workspace = TestWorkspace::new("supervisor-group-reap");
    let child_pid_file = workspace.child("child.pid").expect("child pid path");
    let grandchild_pid_file = workspace
        .child("grandchild.pid")
        .expect("grandchild pid path");
    let command = ProcessSpec::new("/bin/sh").args([
        "-c",
        &format!(
            "printf %s \"$$\" > '{}'; sleep 30 >/dev/null 2>&1 & printf %s \"$!\" > '{}'; wait",
            child_pid_file.display(),
            grandchild_pid_file.display()
        ),
    ]);
    let config = ProcessConfig::default()
        .with_timeout(Some(Duration::from_millis(50)))
        .with_lifecycle_policy(
            LifecyclePolicy::default().with_grace_period(Duration::from_secs(1)),
        );

    let result = run_with_cancel(&command, &config, CancellationToken::new())
        .await
        .expect("run completes");

    let child_pid = read_pid(&child_pid_file);
    let grandchild_pid = read_pid(&grandchild_pid_file);
    assert!(result.timed_out);
    assert!(!pid_exists(child_pid));
    assert!(!pid_exists(grandchild_pid));
}

#[tokio::test]
#[cfg(unix)]
async fn ignored_sigterm_escalates_to_sigkill_after_grace() {
    let command =
        ProcessSpec::new("/bin/sh").args(["-c", "trap '' TERM; while :; do sleep 1; done"]);
    let config = ProcessConfig::default()
        .with_timeout(Some(Duration::from_millis(50)))
        .with_lifecycle_policy(
            LifecyclePolicy::default().with_grace_period(Duration::from_millis(20)),
        );

    let result = run_with_cancel(&command, &config, CancellationToken::new())
        .await
        .expect("run completes");

    assert!(result.timed_out);
    assert!(result.stderr.contains("SIGKILL"));
}

#[tokio::test]
#[cfg(unix)]
async fn dropping_run_future_mid_flight_reaps_child_group() {
    let workspace = TestWorkspace::new("supervisor-drop-future");
    let pid_file = workspace.child("child.pid").expect("pid path");
    let command = ProcessSpec::new("/bin/sh").args([
        "-c",
        &format!(
            "printf %s \"$$\" > '{}'; while :; do sleep 1; done",
            pid_file.display()
        ),
    ]);
    let config = ProcessConfig::default().with_timeout(None);
    let handle = tokio::spawn(async move {
        let _ = run_with_cancel(&command, &config, CancellationToken::new()).await;
    });

    let pid = read_pid_async(&pid_file).await;
    handle.abort();
    let _ = handle.await;
    for _ in 0..50 {
        if !pid_exists(pid) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(
        !pid_exists(pid),
        "aborted run future must not leave a child alive"
    );
}

#[test]
#[cfg(unix)]
fn panic_while_child_is_live_reaps_via_supervisor_drop() {
    let workspace = TestWorkspace::new("supervisor-panic-drop");
    let pid_file = workspace.child("child.pid").expect("pid path");
    let pid_path = pid_file.clone();
    let result = std::panic::catch_unwind(move || {
        let supervisor = ProcessSupervisor::new(LifecyclePolicy::default());
        let mut command = std::process::Command::new("/bin/sh");
        command
            .args([
                "-c",
                &format!(
                    "printf %s \"$$\" > '{}'; while :; do sleep 1; done",
                    pid_path.display()
                ),
            ])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        let _child = supervisor
            .spawn_blocking(&mut command)
            .expect("child spawns");
        while !pid_path.exists() {
            std::thread::yield_now();
        }
        panic!("force unwind");
    });
    assert!(result.is_err());
    let pid = read_pid(&pid_file);
    assert!(!pid_exists(pid));
}

#[tokio::test]
#[cfg(unix)]
async fn shutdown_reaps_all_tracked_groups_concurrently() {
    let supervisor = ProcessSupervisor::new(
        LifecyclePolicy::default().with_grace_period(Duration::from_secs(1)),
    );
    let mut children = Vec::new();
    for _ in 0..3 {
        let mut command = tokio::process::Command::new("/bin/sh");
        command
            .args([
                "-c",
                "trap 'sleep 1; exit 0' TERM; while :; do sleep 1; done",
            ])
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());
        children.push(supervisor.spawn_async(&mut command).expect("child spawns"));
    }
    assert_eq!(supervisor.registry_len(), 3);
    let started = std::time::Instant::now();
    supervisor
        .shutdown("test shutdown")
        .await
        .expect("shutdown succeeds");
    assert!(started.elapsed() < Duration::from_secs(2));
    assert_eq!(supervisor.registry_len(), 0);
    drop(children);
}

#[tokio::test]
#[cfg(unix)]
async fn lifecycle_policy_isolation_changes_descendant_cleanup_behavior() {
    let workspace = TestWorkspace::new("supervisor-policy");
    let kept_pid_file = workspace.child("kept.pid").expect("pid path");
    let command = ProcessSpec::new("/bin/sh").args([
        "-c",
        &format!(
            "sleep 30 >/dev/null 2>&1 & printf %s \"$!\" > '{}'; exit 0",
            kept_pid_file.display()
        ),
    ]);
    let config = ProcessConfig::default()
        .with_timeout(None)
        .with_lifecycle_policy(
            LifecyclePolicy::default()
                .with_isolate_process_group(false)
                .with_terminate_descendants(false),
        );
    let result = run_with_cancel(&command, &config, CancellationToken::new())
        .await
        .expect("run succeeds");
    assert!(result.success());
    let pid = read_pid(&kept_pid_file);
    assert!(pid_exists(pid));
    // SAFETY: the test intentionally cleans up the process that the non-descendant policy left alive.
    unsafe {
        libc::kill(i32::try_from(pid).expect("pid fits"), libc::SIGKILL);
    }
}

#[tokio::test]
#[cfg(unix)]
async fn zero_or_exited_pid_is_success_shaped() {
    let supervisor = ProcessSupervisor::new(LifecyclePolicy::default());
    let _guard = supervisor.track_pid(0);
    supervisor
        .shutdown("zero pid")
        .await
        .expect("zero pid is okay");
}
