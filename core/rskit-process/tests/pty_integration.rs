//! Behavioral tests for pseudoterminal-backed execution.
//!
//! PTY mode is Unix-only, so the whole file is gated on `cfg(unix)`.

#![cfg(unix)]

use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use rskit_process::{
    ErrorCode, InputPolicy, OutputObserver, OutputPolicy, ProcessConfig, ProcessIo, ProcessSpec,
    PtyIo, PtySize, SignalPolicy, run_with_cancel,
};
use tokio_util::sync::CancellationToken;

fn pty_config(io: PtyIo) -> ProcessConfig {
    ProcessConfig::default()
        .with_timeout(Some(Duration::from_secs(30)))
        .with_io(ProcessIo::pty(io))
}

#[tokio::test]
async fn a_missing_program_on_a_pty_is_a_typed_not_found_spawn_failure() {
    // PTY mode must classify a missing program the same way the pipe runners do:
    // a NotFound spawn failure carrying a spawn-failure message, not an opaque
    // internal error.
    let spec = ProcessSpec::new("/definitely/not/rskit-process-missing");
    let error = run_with_cancel(
        &spec,
        &pty_config(PtyIo::default()),
        CancellationToken::new(),
    )
    .await
    .expect_err("a missing program must fail to spawn");

    assert_eq!(error.code(), ErrorCode::NotFound);
    assert!(
        error.to_string().contains("failed to spawn process"),
        "the error must still indicate a spawn failure: {error}"
    );
}

#[tokio::test]
async fn child_sees_a_real_terminal_on_stdout() {
    // Without a PTY this prints `notty`; on a PTY it prints `tty`.
    let spec = ProcessSpec::new("/bin/sh")
        .args(["-c", "if [ -t 1 ]; then printf tty; else printf notty; fi"]);
    let result = run_with_cancel(
        &spec,
        &pty_config(PtyIo::default()),
        CancellationToken::new(),
    )
    .await
    .expect("run");

    assert_eq!(result.stdout, "tty");
    assert_eq!(result.exit_code, Some(0));
    assert!(result.success());
}

#[tokio::test]
async fn stdout_and_stderr_merge_into_the_single_terminal_stream() {
    // Both streams share the terminal; stderr is empty in PTY mode.
    let spec = ProcessSpec::new("/bin/sh").args(["-c", "printf out; printf err 1>&2"]);
    let result = run_with_cancel(
        &spec,
        &pty_config(PtyIo::default()),
        CancellationToken::new(),
    )
    .await
    .expect("run");

    assert!(result.stdout.contains("out"));
    assert!(result.stdout.contains("err"));
    assert_eq!(result.stderr, "");
}

#[tokio::test]
async fn window_size_is_advertised_to_the_child() {
    // `stty size` reports "<rows> <cols>" from the controlling terminal.
    let spec = ProcessSpec::new("/bin/sh").args(["-c", "stty size"]);
    let io = PtyIo::default().with_size(PtySize::new(42, 133));
    let result = run_with_cancel(&spec, &pty_config(io), CancellationToken::new())
        .await
        .expect("run");

    assert_eq!(result.stdout.trim(), "42 133");
}

#[tokio::test]
async fn merged_output_is_observed_live() {
    let chunks = Arc::new(Mutex::new(Vec::<u8>::new()));
    let observer = {
        let chunks = Arc::clone(&chunks);
        OutputObserver::new().with_stdout_bytes(move |bytes| chunks.lock().extend_from_slice(bytes))
    };
    let io = PtyIo::new(observer).with_output(OutputPolicy::observe_only());

    let spec = ProcessSpec::new("/bin/sh").args(["-c", "printf hello-pty"]);
    let result = run_with_cancel(&spec, &pty_config(io), CancellationToken::new())
        .await
        .expect("run");

    // observe_only: nothing retained in the result, everything seen live.
    assert_eq!(result.stdout, "");
    assert!(
        String::from_utf8_lossy(&chunks.lock()).contains("hello-pty"),
        "observer must receive the merged stream"
    );
}

#[tokio::test]
async fn predefined_stdin_bytes_reach_the_child() {
    // PTY stdin is line-discipline based: input is delivered as if typed, and a
    // reader returns on the newline (there is no pipe-style EOF-on-close).
    let spec = ProcessSpec::new("/bin/sh").args(["-c", "read line; printf 'got:%s' \"$line\""]);
    let io = PtyIo::default().with_input(InputPolicy::Bytes(b"piped-in\n".to_vec()));
    let result = run_with_cancel(&spec, &pty_config(io), CancellationToken::new())
        .await
        .expect("run");

    assert!(result.stdout.contains("got:piped-in"));
    assert_eq!(result.exit_code, Some(0));
}

#[tokio::test]
async fn nonzero_exit_is_reported() {
    let spec = ProcessSpec::new("/bin/sh").args(["-c", "exit 3"]);
    let result = run_with_cancel(
        &spec,
        &pty_config(PtyIo::default()),
        CancellationToken::new(),
    )
    .await
    .expect("run");

    assert_eq!(result.exit_code, Some(3));
    assert!(!result.success());
}

#[tokio::test]
async fn slow_child_is_terminated_on_timeout() {
    let spec = ProcessSpec::new("/bin/sh").args(["-c", "sleep 30"]);
    let config = ProcessConfig::default()
        .with_timeout(Some(Duration::from_millis(150)))
        .with_io(ProcessIo::pty(PtyIo::default()));

    let result = run_with_cancel(&spec, &config, CancellationToken::new())
        .await
        .expect("run");

    assert!(result.timed_out, "slow child must be marked timed out");
}

#[tokio::test]
async fn cancellation_terminates_the_child() {
    let spec = ProcessSpec::new("/bin/sh").args(["-c", "sleep 30"]);
    let config = ProcessConfig::default()
        .with_timeout(None)
        .with_io(ProcessIo::pty(PtyIo::default()));
    let cancel = CancellationToken::new();

    let handle = {
        let cancel = cancel.clone();
        tokio::spawn(async move { run_with_cancel(&spec, &config, cancel).await })
    };
    tokio::time::sleep(Duration::from_millis(100)).await;
    cancel.cancel();

    let result = handle.await.expect("join").expect("run");
    assert!(result.cancelled, "cancelled child must be marked cancelled");
}

#[tokio::test]
async fn inherited_stdin_is_rejected_for_pty_mode() {
    let spec = ProcessSpec::new("/bin/cat");
    let io = PtyIo::default().with_input(InputPolicy::Inherit);
    let error = run_with_cancel(&spec, &pty_config(io), CancellationToken::new())
        .await
        .expect_err("inherited stdin must be rejected");

    assert_eq!(error.code(), rskit_process::ErrorCode::InvalidInput);
}

#[tokio::test]
async fn disabling_the_process_group_is_rejected_for_pty_mode() {
    // PTY setup must call `setsid()` to own the terminal, which always starts a
    // new session/process group; `create_process_group = false` cannot be
    // honored and must be rejected rather than silently ignored.
    let spec = ProcessSpec::new("/bin/cat");
    let config = pty_config(PtyIo::default())
        .with_signal_policy(SignalPolicy::default().with_create_process_group(false));
    let error = run_with_cancel(&spec, &config, CancellationToken::new())
        .await
        .expect_err("disabling the process group must be rejected in PTY mode");

    assert_eq!(error.code(), rskit_process::ErrorCode::InvalidInput);
}

#[tokio::test]
async fn capturing_only_stderr_still_retains_the_merged_stream() {
    // stdout and stderr are one stream on a PTY, so requesting stderr capture
    // (with stdout capture off) must still retain the combined output.
    let mut policy = OutputPolicy::observe_only();
    policy.capture_stderr = true;
    let io = PtyIo::default().with_output(policy);
    let spec = ProcessSpec::new("/bin/sh").args(["-c", "printf out; printf err 1>&2"]);
    let result = run_with_cancel(&spec, &pty_config(io), CancellationToken::new())
        .await
        .expect("run");

    assert!(result.stdout.contains("out"));
    assert!(result.stdout.contains("err"));
}
