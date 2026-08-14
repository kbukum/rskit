#![allow(missing_docs)]

use rskit_cli::{ShutdownController, ShutdownPolicy};

#[cfg(unix)]
use std::io::{BufRead, BufReader, Write};
#[cfg(unix)]
use std::num::NonZeroI32;
#[cfg(unix)]
use std::process::{Command, ExitStatus, Stdio};
#[cfg(unix)]
use std::time::Duration;

#[cfg(unix)]
use rskit_cli::ShutdownSignal;
#[cfg(unix)]
use rskit_errors::ErrorCode;

const CHILD_MODE: &str = "RSKIT_CLI_SHUTDOWN_CHILD";
#[cfg(unix)]
const SIGNAL_NAME: &str = "RSKIT_CLI_SHUTDOWN_SIGNAL";
#[cfg(unix)]
const CHILD_READY: &str = "ready";
#[cfg(unix)]
const CHILD_CANCELLED: &str = "cancelled";
#[cfg(unix)]
const EXIT_CODE: i32 = 77;

#[cfg(unix)]
#[test]
fn token_is_uncancelled_at_rest_and_cancels_on_sigint() {
    assert_first_signal_cancels("sigint");
}

#[cfg(unix)]
#[test]
fn token_is_uncancelled_at_rest_and_cancels_on_sigterm() {
    assert_first_signal_cancels("sigterm");
}

#[cfg(unix)]
#[test]
fn token_is_uncancelled_at_rest_and_cancels_on_sighup() {
    assert_first_signal_cancels("sighup");
}

#[cfg(unix)]
#[test]
fn second_signal_after_cancellation_force_exits_with_configured_code() {
    let mut child = spawn_child("second-signal", Some("sigterm"));
    let mut stdout = child_stdout(&mut child);
    assert_ready(&mut stdout);

    send_signal(child.id(), "sigterm");
    assert_cancelled(&mut stdout);
    send_signal(child.id(), "sigterm");

    assert_exit_code(child.wait().expect("child wait"), EXIT_CODE);
}

#[cfg(unix)]
#[test]
fn drain_deadline_force_exits_when_drain_outlives_deadline() {
    let mut child = spawn_child("drain-deadline", Some("sigint"));
    let mut stdout = child_stdout(&mut child);
    assert_ready(&mut stdout);

    send_signal(child.id(), "sigint");
    assert_cancelled(&mut stdout);

    assert_exit_code(child.wait().expect("child wait"), EXIT_CODE);
}

#[cfg(unix)]
#[tokio::test]
async fn signal_stream_install_failure_surfaces_typed_app_error() {
    let err = ShutdownController::install(
        ShutdownPolicy::default().with_signals([ShutdownSignal::unix_raw(0)]),
    )
    .expect_err("invalid signal should fail to install");

    assert_eq!(err.code(), ErrorCode::Internal);
    assert!(err.message().contains("install shutdown signal stream"));
}

#[tokio::test]
async fn cancellation_propagates_to_clones() {
    let controller = ShutdownController::install(ShutdownPolicy::default())
        .expect("shutdown controller should install");
    let token = controller.token();
    let clone = token.clone();

    assert!(!token.is_cancelled());
    assert!(!clone.is_cancelled());

    token.cancel();
    token.cancelled().await;

    assert!(token.is_cancelled());
    assert!(clone.is_cancelled());
}

#[test]
fn shutdown_harness_entrypoint() {
    let Ok(mode) = std::env::var(CHILD_MODE) else {
        return;
    };

    #[cfg(unix)]
    {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("test runtime");
        runtime.block_on(async move {
            match mode.as_str() {
                "first-signal" => child_first_signal().await,
                "second-signal" => child_second_signal().await,
                "drain-deadline" => child_drain_deadline().await,
                other => panic!("unknown child mode: {other}"),
            }
        });
    }
    #[cfg(not(unix))]
    {
        let _ = mode;
    }
}

#[cfg(unix)]
fn assert_first_signal_cancels(signal_name: &str) {
    let mut child = spawn_child("first-signal", Some(signal_name));
    let mut stdout = child_stdout(&mut child);
    assert_ready(&mut stdout);

    send_signal(child.id(), signal_name);
    assert_cancelled(&mut stdout);

    assert!(child.wait().expect("child wait").success());
}

#[cfg(unix)]
fn child_signal() -> ShutdownSignal {
    match std::env::var(SIGNAL_NAME).expect("signal env").as_str() {
        "sigint" => ShutdownSignal::interrupt(),
        "sigterm" => ShutdownSignal::terminate(),
        "sighup" => ShutdownSignal::hangup(),
        other => panic!("unknown signal name: {other}"),
    }
}

#[cfg(unix)]
async fn child_first_signal() {
    let controller =
        ShutdownController::install(ShutdownPolicy::default().with_signals([child_signal()]))
            .expect("shutdown controller should install");
    assert!(!controller.token().is_cancelled());

    child_emit(CHILD_READY);
    controller.token().cancelled().await;
    child_emit(CHILD_CANCELLED);
}

#[cfg(unix)]
async fn child_second_signal() {
    let controller = ShutdownController::install(
        ShutdownPolicy::default()
            .with_signals([child_signal()])
            .with_second_signal_exit_code(NonZeroI32::new(EXIT_CODE).expect("non-zero exit code")),
    )
    .expect("shutdown controller should install");
    assert!(!controller.token().is_cancelled());

    child_emit(CHILD_READY);
    controller.token().cancelled().await;
    child_emit(CHILD_CANCELLED);
    std::future::pending::<()>().await;
}

#[cfg(unix)]
async fn child_drain_deadline() {
    tokio::time::pause();
    let controller = ShutdownController::install(
        ShutdownPolicy::default()
            .with_signals([child_signal()])
            .with_drain_deadline(Duration::from_secs(1))
            .with_second_signal_exit_code(NonZeroI32::new(EXIT_CODE).expect("non-zero exit code")),
    )
    .expect("shutdown controller should install");
    assert!(!controller.token().is_cancelled());

    child_emit(CHILD_READY);
    controller.token().cancelled().await;
    child_emit(CHILD_CANCELLED);
    tokio::time::advance(Duration::from_secs(1)).await;
    std::future::pending::<()>().await;
}

#[cfg(unix)]
fn spawn_child(mode: &str, signal_name: Option<&str>) -> std::process::Child {
    let mut command = Command::new(std::env::current_exe().expect("current test executable"));
    command
        .arg("--exact")
        .arg("shutdown_harness_entrypoint")
        .arg("--nocapture")
        .arg("--quiet")
        .env(CHILD_MODE, mode)
        .stdout(Stdio::piped());
    if let Some(signal_name) = signal_name {
        command.env(SIGNAL_NAME, signal_name);
    }
    command.spawn().expect("spawn shutdown harness")
}

#[cfg(unix)]
fn child_stdout(child: &mut std::process::Child) -> BufReader<std::process::ChildStdout> {
    BufReader::new(child.stdout.take().expect("child stdout"))
}

#[cfg(unix)]
fn assert_ready(stdout: &mut impl BufRead) {
    assert_child_line(stdout, CHILD_READY);
}

#[cfg(unix)]
fn assert_cancelled(stdout: &mut impl BufRead) {
    assert_child_line(stdout, CHILD_CANCELLED);
}

#[cfg(unix)]
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

#[cfg(unix)]
fn send_signal(pid: u32, signal: &str) {
    let status = Command::new("kill")
        .arg("-s")
        .arg(signal.trim_start_matches("sig"))
        .arg(pid.to_string())
        .status()
        .expect("run kill command");
    assert!(status.success(), "failed to send {signal} to pid {pid}");
}

#[cfg(unix)]
fn assert_exit_code(status: ExitStatus, expected: i32) {
    assert_eq!(status.code(), Some(expected));
}

#[cfg(unix)]
fn child_emit(line: &str) {
    let mut stdout = std::io::stdout().lock();
    writeln!(stdout, "{line}").expect("write child line");
    stdout.flush().expect("flush child line");
}
