//! Seam behavior between the process supervisor and the CLI shutdown controller.
//!
//! Force-exit (a second signal) must win over a slow/wedged supervisor backstop: the
//! controller's escalation path is independent of the fan-out and always able to fire.

#![cfg(all(unix, feature = "process", feature = "cli"))]

use std::io::{BufRead, BufReader, Write};
use std::num::NonZeroI32;
use std::process::{Command, ExitStatus, Stdio};
use std::time::Duration;

const CHILD_MODE: &str = "RSKIT_SEAM_SHUTDOWN_CHILD";
const CHILD_READY: &str = "ready";
const CHILD_CANCELLED: &str = "cancelled";
const FORCE_EXIT_CODE: i32 = 79;

#[test]
fn second_signal_force_exit_wins_over_wedged_backstop() {
    let mut child = spawn_harness("wedged-backstop");
    let mut stdout = BufReader::new(child.stdout.take().expect("child stdout"));

    assert_child_line(&mut stdout, CHILD_READY);

    // First signal: cooperative cancel trips, which starts the (wedged) supervisor backstop.
    send_sigterm(child.id());
    assert_child_line(&mut stdout, CHILD_CANCELLED);

    // Second signal: force-exit must win immediately, without waiting for the backstop.
    send_sigterm(child.id());
    assert_force_exit_within(&mut child, Duration::from_secs(5));
}

/// Child harness: install the controller, wire a deliberately wedged supervisor backstop.
#[test]
fn seam_shutdown_harness_entrypoint() {
    let Ok(mode) = std::env::var(CHILD_MODE) else {
        return;
    };

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("test runtime");
    runtime.block_on(async move {
        match mode.as_str() {
            "wedged-backstop" => child_wedged_backstop().await,
            other => panic!("unknown child mode: {other}"),
        }
    });
}

async fn child_wedged_backstop() {
    use rskit::cli::{ShutdownController, ShutdownPolicy, ShutdownSignal};
    use rskit::process::{LifecyclePolicy, ProcessSupervisor};

    let controller = ShutdownController::install(
        ShutdownPolicy::default()
            .with_signals([ShutdownSignal::terminate()])
            .with_second_signal_exit_code(
                NonZeroI32::new(FORCE_EXIT_CODE).expect("non-zero exit code"),
            ),
    )
    .expect("shutdown controller installs");

    // A 60s grace over a child that ignores SIGTERM keeps the backstop wedged well past the
    // test window, so only the controller's force-exit can end the process promptly.
    let supervisor = ProcessSupervisor::new(
        LifecyclePolicy::default().with_grace_period(Duration::from_secs(90)),
    );
    let mut command = tokio::process::Command::new("/bin/sh");
    command
        .args(["-c", "trap '' TERM; sleep 30"])
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    let _child = supervisor.spawn_async(&mut command).expect("child spawns");
    let _subscription = supervisor
        .subscribe_shutdown(controller.token())
        .expect("subscribe within runtime");

    emit(CHILD_READY);
    controller.token().cancelled().await;
    emit(CHILD_CANCELLED);
    std::future::pending::<()>().await;
}

fn spawn_harness(mode: &str) -> std::process::Child {
    let mut command = Command::new(std::env::current_exe().expect("current test executable"));
    command
        .arg("--exact")
        .arg("seam_shutdown_harness_entrypoint")
        .arg("--nocapture")
        .arg("--quiet")
        .env(CHILD_MODE, mode)
        .stdout(Stdio::piped());
    command.spawn().expect("spawn seam harness")
}

fn assert_child_line(stdout: &mut impl BufRead, expected: &str) {
    loop {
        let mut line = String::new();
        stdout.read_line(&mut line).expect("child output line");
        assert_ne!(line, "", "child exited before emitting {expected}");
        if line.trim() == expected {
            return;
        }
    }
}

fn send_sigterm(pid: u32) {
    let status = Command::new("kill")
        .arg("-s")
        .arg("TERM")
        .arg(pid.to_string())
        .status()
        .expect("run kill command");
    assert!(status.success(), "failed to send SIGTERM to pid {pid}");
}

/// Poll for the child's forced exit within `budget`, failing (not hanging) on regression.
fn assert_force_exit_within(child: &mut std::process::Child, budget: Duration) {
    let deadline = std::time::Instant::now() + budget;
    loop {
        match child.try_wait().expect("poll child") {
            Some(status) => {
                assert_exit_code(status, FORCE_EXIT_CODE);
                return;
            }
            None if std::time::Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                panic!("force-exit did not win over the wedged backstop within {budget:?}");
            }
            None => std::thread::sleep(Duration::from_millis(20)),
        }
    }
}

fn assert_exit_code(status: ExitStatus, expected: i32) {
    assert_eq!(status.code(), Some(expected));
}

fn emit(line: &str) {
    let mut stdout = std::io::stdout().lock();
    writeln!(stdout, "{line}").expect("write child line");
    stdout.flush().expect("flush child line");
}
