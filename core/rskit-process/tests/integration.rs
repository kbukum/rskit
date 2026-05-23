use std::time::Duration;

use parking_lot::Mutex;
use rskit_process::{Command, ErrorCode, OutputObserver, ProcessConfig, run_with_cancel};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

#[tokio::test]
async fn runs_command_and_captures_stdout() {
    let command = Command::new("/usr/bin/printf").args(["%s", "hello"]);
    let result = run_with_cancel(
        &command,
        &ProcessConfig::default(),
        CancellationToken::new(),
    )
    .await
    .unwrap();

    assert_eq!(result.stdout, "hello");
    assert_eq!(result.stderr, "");
    assert_eq!(result.exit_code, Some(0));
    assert!(result.success());
}

#[tokio::test]
async fn writes_stdin_to_process() {
    let command = Command::new("/bin/cat").stdin(b"echoed".to_vec());
    let result = run_with_cancel(
        &command,
        &ProcessConfig::default(),
        CancellationToken::new(),
    )
    .await
    .unwrap();

    assert_eq!(result.stdout, "echoed");
}

#[tokio::test]
async fn scrub_env_starts_with_empty_environment() {
    let command = Command::new("/usr/bin/env")
        .env("ONLY_ME", "present")
        .scrub_env();
    let result = run_with_cancel(
        &command,
        &ProcessConfig::default(),
        CancellationToken::new(),
    )
    .await
    .unwrap();

    assert!(result.stdout.contains("ONLY_ME=present"));
    assert!(!result.stdout.contains("PATH="));
}

#[tokio::test]
async fn max_output_bytes_limits_captured_output() {
    let payload = "x".repeat(256);
    let command = Command::new("/usr/bin/printf").args(["%s", payload.as_str()]);
    let config = ProcessConfig {
        max_output_bytes: Some(32),
        ..ProcessConfig::default()
    };

    let result = run_with_cancel(&command, &config, CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(result.stdout.len(), 32);
    assert!(result.stdout.chars().all(|ch| ch == 'x'));
}

#[tokio::test]
async fn observer_handles_non_utf8_output_lossily() {
    let observed = Arc::new(Mutex::new(Vec::new()));
    let command = Command::new("/usr/bin/printf").args(["%b", "\\377\\n"]);
    let result = rskit_process::run_with_observer(
        &command,
        &ProcessConfig::default(),
        CancellationToken::new(),
        OutputObserver::new().with_stdout_line({
            let observed = Arc::clone(&observed);
            move |line| observed.lock().push(line.to_string())
        }),
    )
    .await
    .unwrap();

    assert!(result.success());
    assert_eq!(observed.lock().as_slice(), ["�"]);
}

#[tokio::test]
async fn observer_caps_long_lines_before_newline() {
    let observed = Arc::new(Mutex::new(Vec::new()));
    let command = Command::new("/usr/bin/printf").args(["%s", "x".repeat(128).as_str()]);
    let config = ProcessConfig {
        max_output_bytes: Some(32),
        ..ProcessConfig::default()
    };

    let result = rskit_process::run_with_observer(
        &command,
        &config,
        CancellationToken::new(),
        OutputObserver::new().with_stdout_line({
            let observed = Arc::clone(&observed);
            move |line| observed.lock().push(line.to_string())
        }),
    )
    .await
    .unwrap();

    assert!(result.success());
    assert_eq!(result.stdout.len(), 32);
    assert_eq!(observed.lock().as_slice(), ["x".repeat(32)]);
}

#[tokio::test]
async fn observer_runs_when_capture_output_is_disabled() {
    let observed = Arc::new(Mutex::new(Vec::new()));
    let command = Command::new("/usr/bin/printf").args(["observed\\n"]);
    let config = ProcessConfig {
        capture_output: false,
        ..ProcessConfig::default()
    };

    let result = rskit_process::run_with_observer(
        &command,
        &config,
        CancellationToken::new(),
        OutputObserver::new().with_stdout_line({
            let observed = Arc::clone(&observed);
            move |line| observed.lock().push(line.to_string())
        }),
    )
    .await
    .unwrap();

    assert!(result.success());
    assert!(result.stdout.is_empty());
    assert!(result.stdout_bytes.is_empty());
    assert_eq!(observed.lock().as_slice(), ["observed"]);
}

#[tokio::test]
async fn observer_treats_carriage_return_as_line_boundary() {
    let observed = Arc::new(Mutex::new(Vec::new()));
    let command = Command::new("/usr/bin/printf").args(["one\\rtwo\\r\\nthree\\n"]);

    let result = rskit_process::run_with_observer(
        &command,
        &ProcessConfig::default(),
        CancellationToken::new(),
        OutputObserver::new().with_stdout_line({
            let observed = Arc::clone(&observed);
            move |line| observed.lock().push(line.to_string())
        }),
    )
    .await
    .unwrap();

    assert!(result.success());
    assert_eq!(observed.lock().as_slice(), ["one", "two", "three"]);
}

#[tokio::test]
async fn timeout_escalates_and_marks_result() {
    let command = Command::new("/bin/sh").args(["-c", "printf 123456789abcdef >&2; sleep 2"]);
    let config = ProcessConfig {
        timeout: Some(Duration::from_millis(50)),
        grace_period: Duration::from_millis(10),
        max_output_bytes: Some(8),
        ..ProcessConfig::default()
    };

    let result = run_with_cancel(&command, &config, CancellationToken::new())
        .await
        .unwrap();
    assert!(result.timed_out);
    assert!(result.exit_code.is_none() || result.exit_code != Some(0));
    assert!(result.stderr_bytes.len() <= 8);
    assert!(result.stderr_truncated);
}

#[tokio::test]
async fn argv_only_execution_prevents_shell_injection() {
    let command = Command::new("/usr/bin/printf").args(["%s", "$(echo injected); rm -rf /"]);
    let result = run_with_cancel(
        &command,
        &ProcessConfig::default(),
        CancellationToken::new(),
    )
    .await
    .unwrap();

    assert_eq!(result.stdout, "$(echo injected); rm -rf /");
}

#[tokio::test]
async fn process_result_check_reports_failures() {
    let command = Command::new("/usr/bin/false");
    let result = run_with_cancel(
        &command,
        &ProcessConfig::default(),
        CancellationToken::new(),
    )
    .await
    .unwrap();

    assert!(result.check().is_err());
}

#[tokio::test]
async fn cancellation_terminates_process() {
    let command = Command::new("/bin/sleep").arg("2");
    let cancel = CancellationToken::new();
    cancel.cancel();

    let result = run_with_cancel(&command, &ProcessConfig::default(), cancel).await;
    let error = result.expect_err("cancellation should fail");
    assert_eq!(error.code, ErrorCode::Cancelled);
    assert!(error.details().contains_key("duration_ms"));
    assert!(error.details().contains_key("stdout"));
    assert!(error.details().contains_key("stderr"));
}
