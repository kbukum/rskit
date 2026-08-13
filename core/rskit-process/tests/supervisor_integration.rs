#![allow(missing_docs)]

use std::time::Duration;

use rskit_process::{
    LifecyclePolicy, ProcessConfig, ProcessSpec, ProcessSupervisor, run_with_cancel,
};
use rskit_testutil::TestWorkspace;
#[cfg(target_os = "linux")]
use tokio::io::AsyncReadExt;
use tokio_util::sync::CancellationToken;

#[cfg(unix)]
fn pid_exists(pid: u32) -> bool {
    let Ok(pid) = i32::try_from(pid) else {
        return false;
    };
    // SAFETY: signal 0 performs an existence check without delivering a signal.
    unsafe { libc::kill(pid, 0) == 0 }
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
            "printf %s \"$$\" > '{}'; (printf %s \"$!\" > '{}'; sleep 30) & wait",
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

    let child_pid: u32 = std::fs::read_to_string(child_pid_file)
        .expect("child pid")
        .trim()
        .parse()
        .expect("numeric child pid");
    let grandchild_pid: u32 = std::fs::read_to_string(grandchild_pid_file)
        .expect("grandchild pid")
        .trim()
        .parse()
        .expect("numeric grandchild pid");
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

    while !pid_file.exists() {
        tokio::task::yield_now().await;
    }
    let pid: u32 = std::fs::read_to_string(pid_file)
        .expect("pid")
        .trim()
        .parse()
        .expect("numeric pid");
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
    let pid: u32 = std::fs::read_to_string(pid_file)
        .expect("pid")
        .trim()
        .parse()
        .expect("numeric pid");
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
#[cfg(target_os = "linux")]
async fn linux_parent_death_sigkills_isolated_child() {
    let mut parent = tokio::process::Command::new(std::env::current_exe().expect("current exe"));
    parent
        .args([
            "--exact",
            "linux_parent_death_helper",
            "--nocapture",
            "--test-threads=1",
        ])
        .env("RSKIT_PDEATH_HELPER", "1")
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::null());
    let mut child = parent.spawn().expect("parent spawns");
    let mut stdout = child.stdout.take().expect("stdout");
    let mut bytes = Vec::new();
    stdout.read_to_end(&mut bytes).await.expect("read pid");
    let status = child.wait().await.expect("parent exits");
    assert!(status.success());
    let pid: u32 = String::from_utf8(bytes)
        .expect("utf8")
        .trim()
        .parse()
        .expect("numeric pid");
    for _ in 0..50 {
        if !pid_exists(pid) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(!pid_exists(pid), "parent death should SIGKILL the child");
}

#[test]
#[cfg(target_os = "linux")]
fn linux_parent_death_helper() {
    if std::env::var_os("RSKIT_PDEATH_HELPER").is_none() {
        return;
    }
    let mut command = std::process::Command::new("/bin/sleep");
    command
        .arg("30")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    rskit_process::isolate_process_group(&mut command);
    let child = command.spawn().expect("child spawns");
    println!("{}", child.id());
    std::mem::forget(child);
}

#[tokio::test]
#[cfg(unix)]
async fn lifecycle_policy_isolation_changes_descendant_cleanup_behavior() {
    let workspace = TestWorkspace::new("supervisor-policy");
    let kept_pid_file = workspace.child("kept.pid").expect("pid path");
    let command = ProcessSpec::new("/bin/sh").args([
        "-c",
        &format!(
            "(printf %s \"$!\" > '{}'; sleep 30) & exit 0",
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
    let pid: u32 = std::fs::read_to_string(kept_pid_file)
        .expect("pid")
        .trim()
        .parse()
        .expect("numeric pid");
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
