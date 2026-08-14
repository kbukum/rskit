#![allow(missing_docs)]

use std::ffi::OsString;
use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use rskit_process::{
    ArgRedaction, CapturedIo, EnvPolicy, InheritedIo, InputPolicy, LifecyclePolicy, ObservedIo,
    OutputObserver, OutputPolicy, ProcessConfig, ProcessIo, ProcessResult, ProcessSpec, command,
    interrupt_process_group, kill_process_group, run, run_with_cancel, terminate_process_group,
};
use tokio_util::sync::CancellationToken;

#[test]
fn process_spec_builders_capture_program_args_dir_env_and_policy() {
    let spec = command("echo")
        .arg("hello")
        .args([OsString::from("world")])
        .dir(".")
        .env("TOKEN", "secret")
        .envs([("A", "B")])
        .empty_env();

    assert_eq!(spec.program, std::path::PathBuf::from("echo"));
    assert_eq!(spec.args.len(), 2);
    assert_eq!(spec.dir.as_deref(), Some(std::path::Path::new(".")));
    assert_eq!(spec.env.get("TOKEN").unwrap(), "secret");
    assert_eq!(spec.env_policy, EnvPolicy::Empty);

    let inherited = ProcessSpec::new("env").env_policy(EnvPolicy::Inherit);
    assert_eq!(inherited.env_policy, EnvPolicy::Inherit);
}

#[test]
fn io_and_output_policies_are_explicit_and_chainable() {
    let captured = CapturedIo::new()
        .with_input(InputPolicy::Bytes(b"stdin".to_vec()))
        .with_output(OutputPolicy::captured().with_max_output_bytes(4));
    assert_eq!(captured.input, InputPolicy::Bytes(b"stdin".to_vec()));
    assert_eq!(captured.output.max_output_bytes, Some(4));

    let observed = ObservedIo::new(OutputObserver::default())
        .with_input(InputPolicy::Closed)
        .with_output(OutputPolicy::observe_only().with_unbounded_output());
    assert!(!observed.output.capture_stdout);
    assert_eq!(observed.output.max_output_bytes, None);

    let inherited = InheritedIo::new().with_input(InputPolicy::Inherit);
    assert_eq!(inherited.input, InputPolicy::Inherit);

    assert!(matches!(
        ProcessIo::captured(captured),
        ProcessIo::Captured(_)
    ));
    assert!(matches!(
        ProcessIo::observed(observed),
        ProcessIo::Observed(_)
    ));
    assert!(matches!(
        ProcessIo::inherited(inherited),
        ProcessIo::Inherited(_)
    ));
}

#[test]
fn signal_redaction_and_process_config_builders_update_nested_policy() {
    let signal = LifecyclePolicy::default()
        .with_grace_period(Duration::from_millis(25))
        .with_isolate_process_group(false)
        .with_terminate_descendants(false);
    assert_eq!(signal.grace_period, Duration::from_millis(25));
    assert!(!signal.isolate_process_group);
    assert!(!signal.terminate_descendants);

    let redaction = ArgRedaction::from_names(["password"])
        .with_name("token")
        .with_names(["secret"]);
    assert!(redaction.is_sensitive_arg_name("--password"));
    assert!(redaction.is_sensitive_arg_name("token"));
    assert!(redaction.is_sensitive_arg_name("secret"));

    let config = ProcessConfig::default()
        .with_timeout(None)
        .with_lifecycle_policy(signal)
        .with_arg_redaction(redaction)
        .with_sensitive_arg_name("api-key")
        .with_max_output_bytes(2)
        .with_unbounded_output()
        .with_input(InputPolicy::Bytes(vec![1, 2, 3]));
    assert_eq!(config.timeout, None);
    assert!(config.arg_redaction.is_sensitive_arg_name("api-key"));
    match config.io {
        ProcessIo::Captured(io) => {
            assert_eq!(io.input, InputPolicy::Bytes(vec![1, 2, 3]));
            assert_eq!(io.output.max_output_bytes, None);
        }
        _ => panic!("default process config should be captured"),
    }
}

#[test]
fn blocking_run_validates_unsupported_modes_without_spawning() {
    let empty_program = ProcessSpec::new("");
    let error = run(&empty_program, &ProcessConfig::default()).unwrap_err();
    assert_eq!(error.code(), rskit_process::ErrorCode::InvalidInput);

    let inherited_stdin_in_captured_mode =
        ProcessConfig::default().with_input(InputPolicy::Inherit);
    let error = run(
        &ProcessSpec::new("unused"),
        &inherited_stdin_in_captured_mode,
    )
    .unwrap_err();
    assert_eq!(error.code(), rskit_process::ErrorCode::InvalidInput);

    let observed = ProcessConfig::default().with_io(ProcessIo::observed(ObservedIo::default()));
    let error = run(&ProcessSpec::new("unused"), &observed).unwrap_err();
    assert_eq!(error.code(), rskit_process::ErrorCode::InvalidInput);
}

#[test]
fn process_result_check_reports_all_terminal_states() {
    let ok = ProcessResult::completed(
        Some(0),
        b"stdout".to_vec(),
        b"stderr".to_vec(),
        false,
        false,
        Duration::from_millis(1),
        false,
        false,
    );
    assert!(ok.success());
    assert_eq!(ok.check().unwrap().stdout, "stdout");
    assert_eq!(ok.stderr, "stderr");

    let failed = ProcessResult::completed(
        Some(2),
        Vec::new(),
        Vec::new(),
        false,
        false,
        Duration::ZERO,
        false,
        false,
    );
    assert_eq!(
        failed.check().unwrap_err().code(),
        rskit_process::ErrorCode::Internal
    );

    let killed = ProcessResult::completed(
        None,
        Vec::new(),
        Vec::new(),
        false,
        false,
        Duration::ZERO,
        false,
        false,
    );
    assert_eq!(
        killed.check().unwrap_err().code(),
        rskit_process::ErrorCode::Internal
    );

    let timed_out = ProcessResult::completed(
        None,
        Vec::new(),
        Vec::new(),
        false,
        false,
        Duration::ZERO,
        true,
        false,
    );
    assert_eq!(
        timed_out.check().unwrap_err().code(),
        rskit_process::ErrorCode::Timeout
    );

    let cancelled = ProcessResult::completed(
        None,
        Vec::new(),
        Vec::new(),
        false,
        false,
        Duration::ZERO,
        false,
        true,
    );
    assert_eq!(
        cancelled.check().unwrap_err().code(),
        rskit_process::ErrorCode::Cancelled
    );
}

#[tokio::test]
async fn observed_run_invokes_line_and_byte_callbacks_without_retaining_output() {
    let stdout_lines = Arc::new(Mutex::new(Vec::new()));
    let stderr_lines = Arc::new(Mutex::new(Vec::new()));
    let stdout_bytes = Arc::new(Mutex::new(Vec::new()));
    let stderr_bytes = Arc::new(Mutex::new(Vec::new()));

    let observer = OutputObserver::new()
        .with_stdout_line({
            let lines = Arc::clone(&stdout_lines);
            move |line| lines.lock().push(line.to_string())
        })
        .with_stderr_line({
            let lines = Arc::clone(&stderr_lines);
            move |line| lines.lock().push(line.to_string())
        })
        .with_stdout_bytes({
            let bytes = Arc::clone(&stdout_bytes);
            move |chunk| bytes.lock().extend_from_slice(chunk)
        })
        .with_stderr_bytes({
            let bytes = Arc::clone(&stderr_bytes);
            move |chunk| bytes.lock().extend_from_slice(chunk)
        });
    let config = ProcessConfig::default()
        .with_io(ProcessIo::observed(
            ObservedIo::new(observer).with_output(OutputPolicy::observe_only()),
        ))
        .with_timeout(Some(Duration::from_secs(5)));

    let result = run_with_cancel(
        &ProcessSpec::new("python3").args([
            "-c",
            "import sys; print('out'); print('err', file=sys.stderr)",
        ]),
        &config,
        CancellationToken::new(),
    )
    .await
    .unwrap();

    assert!(result.success());
    assert!(result.stdout.is_empty());
    assert!(result.stderr.is_empty());
    assert_eq!(stdout_lines.lock().as_slice(), ["out"]);
    assert_eq!(stderr_lines.lock().as_slice(), ["err"]);
    assert!(String::from_utf8_lossy(&stdout_bytes.lock()).contains("out"));
    assert!(String::from_utf8_lossy(&stderr_bytes.lock()).contains("err"));
}

#[tokio::test]
async fn async_run_captures_output_input_and_truncation() {
    let result = run_with_cancel(
        &ProcessSpec::new("python3").args([
            "-c",
            "import sys; data=sys.stdin.read(); print(data * 2); print('stderr-data', file=sys.stderr)",
        ]),
        &ProcessConfig::default()
            .with_input(InputPolicy::Bytes(b"abc".to_vec()))
            .with_max_output_bytes(5),
        CancellationToken::new(),
    )
    .await
    .unwrap();

    assert!(result.success());
    assert_eq!(result.stdout, "abcab");
    assert_eq!(result.stderr, "stder");
    assert!(result.stdout_truncated);
    assert!(result.stderr_truncated);
}

#[tokio::test]
async fn async_run_reports_cancelled_process() {
    let cancel = CancellationToken::new();
    let child_cancel = cancel.clone();
    let handle = tokio::spawn(async move {
        run_with_cancel(
            &ProcessSpec::new("python3").args(["-c", "import time; time.sleep(5)"]),
            &ProcessConfig::default()
                .with_timeout(Some(Duration::from_secs(30)))
                .with_lifecycle_policy(
                    LifecyclePolicy::default().with_grace_period(Duration::from_millis(10)),
                ),
            child_cancel,
        )
        .await
    });
    tokio::time::sleep(Duration::from_millis(20)).await;
    cancel.cancel();

    let result = handle.await.unwrap().unwrap();

    assert!(result.cancelled);
    assert!(!result.timed_out);
}

#[tokio::test]
async fn async_inherited_mode_disables_process_group_and_completes() {
    let result = run_with_cancel(
        &ProcessSpec::new("python3").args(["-c", "print('inherited')"]),
        &ProcessConfig::default().with_io(ProcessIo::inherited(InheritedIo::new())),
        CancellationToken::new(),
    )
    .await
    .unwrap();

    assert!(result.success());
    assert!(result.stdout.is_empty());
}

#[test]
fn process_group_public_signals_reject_zero_pid() {
    assert!(!interrupt_process_group(0));
    assert!(!terminate_process_group(0));
    assert!(!kill_process_group(0));
}

#[test]
fn blocking_run_captures_input_output_truncation_and_exit_code() {
    let result = run(
        &ProcessSpec::new("python3").args([
            "-c",
            "import sys; data=sys.stdin.read(); print(data.upper()); print('err-line', file=sys.stderr); sys.exit(3)",
        ]),
        &ProcessConfig::default()
            .with_input(InputPolicy::Bytes(b"abcdef".to_vec()))
            .with_max_output_bytes(4),
    )
    .unwrap();

    assert_eq!(result.exit_code, Some(3));
    assert_eq!(result.stdout, "ABCD");
    assert_eq!(result.stderr, "err-");
    assert!(result.stdout_truncated);
    assert!(result.stderr_truncated);
}

#[test]
fn blocking_run_timeout_marks_result_and_appends_synthetic_stderr() {
    let result = run(
        &ProcessSpec::new("python3").args(["-c", "import time; time.sleep(5)"]),
        &ProcessConfig::default()
            .with_timeout(Some(Duration::from_millis(20)))
            .with_lifecycle_policy(
                LifecyclePolicy::default().with_grace_period(Duration::from_millis(10)),
            )
            .with_unbounded_output(),
    )
    .unwrap();

    assert!(result.timed_out);
    assert!(!result.success());
}
