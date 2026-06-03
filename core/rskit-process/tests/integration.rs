use std::time::Duration;

use parking_lot::Mutex;
use rskit_process::{
    ErrorCode, InheritedIo, InputPolicy, ObservedIo, OutputObserver, OutputPolicy, ProcessConfig,
    ProcessIo, ProcessSpec, run, run_with_cancel,
};
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

#[tokio::test]
async fn runs_command_and_captures_stdout() {
    let command = ProcessSpec::new("/usr/bin/printf").args(["%s", "hello"]);
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
    let command = ProcessSpec::new("/bin/cat");
    let config = ProcessConfig::default().with_input(InputPolicy::Bytes(b"echoed".to_vec()));
    let result = run_with_cancel(&command, &config, CancellationToken::new())
        .await
        .unwrap();

    assert_eq!(result.stdout, "echoed");
}

#[tokio::test]
async fn async_run_observes_timeout_while_writing_stdin() {
    let stdin = vec![b'x'; 2 * 1024 * 1024];
    let command = ProcessSpec::new("/bin/sh").args([
        "-c",
        "dd if=/dev/zero bs=1024 count=256 2>/dev/null; cat >/dev/null; printf done >&2",
    ]);
    let config = ProcessConfig::default()
        .with_timeout(Some(Duration::from_secs(2)))
        .with_input(InputPolicy::Bytes(stdin))
        .with_max_output_bytes(1024);

    let result = run_with_cancel(&command, &config, CancellationToken::new())
        .await
        .unwrap();

    assert!(result.success());
    assert!(result.stdout_truncated);
    assert_eq!(result.stderr, "done");
}

#[tokio::test]
async fn async_run_treats_stdin_broken_pipe_as_success() {
    let command = ProcessSpec::new("/usr/bin/true");
    let config =
        ProcessConfig::default().with_input(InputPolicy::Bytes(vec![b'x'; 2 * 1024 * 1024]));

    let result = run_with_cancel(&command, &config, CancellationToken::new())
        .await
        .unwrap();

    assert!(result.success());
}

#[tokio::test]
async fn scrub_env_starts_with_empty_environment() {
    let command = ProcessSpec::new("/usr/bin/env")
        .env("ONLY_ME", "present")
        .empty_env();
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
    let command = ProcessSpec::new("/usr/bin/printf").args(["%s", payload.as_str()]);
    let config = ProcessConfig::default().with_max_output_bytes(32);

    let result = run_with_cancel(&command, &config, CancellationToken::new())
        .await
        .unwrap();
    assert_eq!(result.stdout.len(), 32);
    assert!(result.stdout.chars().all(|ch| ch == 'x'));
}

#[tokio::test]
async fn observer_handles_non_utf8_output_lossily() {
    let observed = Arc::new(Mutex::new(Vec::new()));
    let command = ProcessSpec::new("/usr/bin/printf").args(["%b", "\\377\\n"]);
    let config = ProcessConfig::default().with_io(ProcessIo::observed(ObservedIo::new(
        OutputObserver::new().with_stdout_line({
            let observed = Arc::clone(&observed);
            move |line| observed.lock().push(line.to_string())
        }),
    )));
    let result = run_with_cancel(&command, &config, CancellationToken::new())
        .await
        .unwrap();

    assert!(result.success());
    assert_eq!(observed.lock().as_slice(), ["�"]);
}

#[tokio::test]
async fn observer_caps_long_lines_before_newline() {
    let observed = Arc::new(Mutex::new(Vec::new()));
    let command = ProcessSpec::new("/usr/bin/printf").args(["%s", "x".repeat(128).as_str()]);
    let config = ProcessConfig::default().with_io(ProcessIo::observed(
        ObservedIo::new(OutputObserver::new().with_stdout_line({
            let observed = Arc::clone(&observed);
            move |line| observed.lock().push(line.to_string())
        }))
        .with_output(OutputPolicy::captured().with_max_output_bytes(32)),
    ));

    let result = run_with_cancel(&command, &config, CancellationToken::new())
        .await
        .unwrap();

    assert!(result.success());
    assert_eq!(result.stdout.len(), 32);
    assert_eq!(observed.lock().as_slice(), ["x".repeat(32)]);
}

#[tokio::test]
async fn observer_runs_when_capture_output_is_disabled() {
    let observed = Arc::new(Mutex::new(Vec::new()));
    let command = ProcessSpec::new("/usr/bin/printf").args(["observed\\n"]);
    let config = ProcessConfig::default().with_io(ProcessIo::observed(
        ObservedIo::new(OutputObserver::new().with_stdout_line({
            let observed = Arc::clone(&observed);
            move |line| observed.lock().push(line.to_string())
        }))
        .with_output(OutputPolicy::observe_only()),
    ));

    let result = run_with_cancel(&command, &config, CancellationToken::new())
        .await
        .unwrap();

    assert!(result.success());
    assert!(result.stdout.is_empty());
    assert!(result.stdout_bytes.is_empty());
    assert_eq!(observed.lock().as_slice(), ["observed"]);
}

#[tokio::test]
async fn observer_forwards_raw_bytes_when_capture_output_is_disabled() {
    let observed = Arc::new(Mutex::new(Vec::new()));
    let command = ProcessSpec::new("/usr/bin/printf").args(["%b", "\\377raw"]);
    let config = ProcessConfig::default().with_io(ProcessIo::observed(
        ObservedIo::new(OutputObserver::new().with_stdout_bytes({
            let observed = Arc::clone(&observed);
            move |bytes| observed.lock().extend_from_slice(bytes)
        }))
        .with_output(OutputPolicy::observe_only()),
    ));

    let result = run_with_cancel(&command, &config, CancellationToken::new())
        .await
        .unwrap();

    assert!(result.success());
    assert!(result.stdout_bytes.is_empty());
    assert_eq!(observed.lock().as_slice(), b"\xffraw");
}

#[tokio::test]
async fn observer_treats_carriage_return_as_line_boundary() {
    let observed = Arc::new(Mutex::new(Vec::new()));
    let command = ProcessSpec::new("/usr/bin/printf").args(["one\\rtwo\\r\\nthree\\n"]);
    let config = ProcessConfig::default().with_io(ProcessIo::observed(ObservedIo::new(
        OutputObserver::new().with_stdout_line({
            let observed = Arc::clone(&observed);
            move |line| observed.lock().push(line.to_string())
        }),
    )));

    let result = run_with_cancel(&command, &config, CancellationToken::new())
        .await
        .unwrap();

    assert!(result.success());
    assert_eq!(observed.lock().as_slice(), ["one", "two", "three"]);
}

#[tokio::test]
async fn timeout_escalates_and_marks_result() {
    let command = ProcessSpec::new("/bin/sh").args(["-c", "printf 123456789abcdef >&2; sleep 2"]);
    let signal =
        rskit_process::SignalPolicy::default().with_grace_period(Duration::from_millis(10));
    let config = ProcessConfig::default()
        .with_timeout(Some(Duration::from_millis(50)))
        .with_signal_policy(signal)
        .with_max_output_bytes(8);

    let result = run_with_cancel(&command, &config, CancellationToken::new())
        .await
        .unwrap();
    assert!(result.timed_out);
    assert!(result.exit_code.is_none() || result.exit_code != Some(0));
    assert!(result.stderr_bytes.len() <= 8);
    assert!(result.stderr_truncated);
}

#[tokio::test]
async fn timeout_does_not_discard_captured_stderr_at_limit() {
    let command = ProcessSpec::new("/bin/sh").args([
        "-c",
        "trap '' TERM; printf 1234567 >&2; while :; do sleep 1; done",
    ]);
    let signal =
        rskit_process::SignalPolicy::default().with_grace_period(Duration::from_millis(10));
    let config = ProcessConfig::default()
        .with_timeout(Some(Duration::from_millis(50)))
        .with_signal_policy(signal)
        .with_max_output_bytes(8);

    let result = run_with_cancel(&command, &config, CancellationToken::new())
        .await
        .unwrap();
    assert!(result.timed_out);
    assert_eq!(result.stderr, "1234567p");
    assert!(result.stderr_truncated);
}

#[tokio::test]
async fn argv_only_execution_prevents_shell_injection() {
    let command = ProcessSpec::new("/usr/bin/printf").args(["%s", "$(echo injected); rm -rf /"]);
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
async fn inherited_mode_does_not_capture_output() {
    let command = ProcessSpec::new("/usr/bin/printf").args(["%s", "terminal"]);
    let config = ProcessConfig::default().with_io(ProcessIo::inherited(InheritedIo::new()));

    let result = run_with_cancel(&command, &config, CancellationToken::new())
        .await
        .unwrap();

    assert!(result.success());
    assert!(result.stdout.is_empty());
    assert!(result.stderr.is_empty());
}

#[tokio::test]
async fn pipe_modes_reject_inherited_stdin() {
    let command = ProcessSpec::new("/bin/cat");
    let config = ProcessConfig::default().with_input(InputPolicy::Inherit);

    let error = run_with_cancel(&command, &config, CancellationToken::new())
        .await
        .expect_err("captured mode should reject inherited stdin");

    assert_eq!(error.code(), ErrorCode::InvalidInput);
}

#[tokio::test]
async fn observed_mode_rejects_inherited_stdin() {
    let command = ProcessSpec::new("/bin/cat");
    let config = ProcessConfig::default().with_io(ProcessIo::observed(
        ObservedIo::new(OutputObserver::new()).with_input(InputPolicy::Inherit),
    ));

    let error = run_with_cancel(&command, &config, CancellationToken::new())
        .await
        .expect_err("observed mode should reject inherited stdin");

    assert_eq!(error.code(), ErrorCode::InvalidInput);
}

#[test]
fn blocking_captured_mode_rejects_inherited_stdin() {
    let command = ProcessSpec::new("/bin/cat");
    let config = ProcessConfig::default().with_input(InputPolicy::Inherit);

    let error =
        run(&command, &config).expect_err("blocking captured mode should reject inherited stdin");

    assert_eq!(error.code(), ErrorCode::InvalidInput);
}

#[tokio::test]
async fn process_result_check_reports_failures() {
    let command = ProcessSpec::new("/usr/bin/false");
    let result = run_with_cancel(
        &command,
        &ProcessConfig::default(),
        CancellationToken::new(),
    )
    .await
    .unwrap();

    assert!(result.check().is_err());
}

#[test]
fn blocking_run_captures_stdout() {
    let command = ProcessSpec::new("/usr/bin/printf").args(["%s", "hello"]);
    let result = run(&command, &ProcessConfig::default()).unwrap();

    assert_eq!(result.stdout, "hello");
    assert_eq!(result.exit_code, Some(0));
    assert!(result.success());
}

#[test]
fn blocking_run_drains_output_while_writing_stdin() {
    let stdin = vec![b'x'; 2 * 1024 * 1024];
    let command = ProcessSpec::new("/bin/sh").args([
        "-c",
        "dd if=/dev/zero bs=1024 count=256 2>/dev/null; cat >/dev/null; printf done >&2",
    ]);
    let config = ProcessConfig::default()
        .with_timeout(Some(Duration::from_secs(2)))
        .with_input(InputPolicy::Bytes(stdin))
        .with_max_output_bytes(1024);

    let result = run(&command, &config).unwrap();

    assert!(result.success());
    assert!(result.stdout_truncated);
    assert_eq!(result.stderr, "done");
}

#[test]
fn blocking_run_preserves_nonzero_exit_code() {
    let command = ProcessSpec::new("/usr/bin/false");
    let result = run(&command, &ProcessConfig::default()).unwrap();

    assert_eq!(result.exit_code, Some(1));
    assert!(result.check().is_err());
}

#[test]
fn blocking_timeout_does_not_discard_captured_stderr_at_limit() {
    let command = ProcessSpec::new("/bin/sh").args([
        "-c",
        "trap '' TERM; printf 1234567 >&2; while :; do sleep 1; done",
    ]);
    let signal =
        rskit_process::SignalPolicy::default().with_grace_period(Duration::from_millis(10));
    let config = ProcessConfig::default()
        .with_timeout(Some(Duration::from_millis(50)))
        .with_signal_policy(signal)
        .with_max_output_bytes(8);

    let result = run(&command, &config).unwrap();
    assert!(result.timed_out);
    assert_eq!(result.stderr, "1234567p");
    assert!(result.stderr_truncated);
}

#[tokio::test]
async fn cancellation_terminates_process() {
    let command = ProcessSpec::new("/bin/sleep").arg("2");
    let cancel = CancellationToken::new();
    cancel.cancel();

    let result = run_with_cancel(&command, &ProcessConfig::default(), cancel)
        .await
        .unwrap();
    assert!(result.cancelled);
}
