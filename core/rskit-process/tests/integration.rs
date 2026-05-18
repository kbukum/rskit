use std::time::Duration;

use rskit_process::{Command, ProcessConfig, run_with_cancel};
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
async fn timeout_escalates_and_marks_result() {
    let command = Command::new("/bin/sleep").arg("2");
    let config = ProcessConfig {
        timeout: Some(Duration::from_millis(50)),
        grace_period: Duration::from_millis(10),
        ..ProcessConfig::default()
    };

    let result = run_with_cancel(&command, &config, CancellationToken::new())
        .await
        .unwrap();
    assert!(result.timed_out);
    assert!(result.exit_code.is_none() || result.exit_code != Some(0));
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
    assert!(result.is_err());
}
