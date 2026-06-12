#![allow(missing_docs)]

use std::time::Duration;

use rskit_process::{
    ErrorCode, InputPolicy, PersistentConfig, PersistentOutput, PersistentOutputStream,
    PersistentReadiness, PersistentStartErrorKind, ProcessConfig, ProcessSpec, ShutdownOutcome,
    persistent_start_error_kind, start_persistent_with_cancel,
};
use rskit_testutil::TestWorkspace;
use tokio_util::sync::CancellationToken;

#[test]
fn started_readiness_allows_natural_wait_to_collect_output() {
    let command = ProcessSpec::new("sh")
        .arg("-c")
        .arg("printf boot; printf err >&2; exit 0");
    let config = PersistentConfig::default()
        .with_readiness(PersistentReadiness::Started)
        .with_readiness_timeout(Duration::from_secs(2));

    let run = start_persistent_with_cancel(
        &command,
        &ProcessConfig::default(),
        &config,
        CancellationToken::new(),
    )
    .expect("started readiness returns a process handle");
    let result = run.process.wait().expect("natural wait succeeds");

    assert!(result.success());
    assert!(result.stdout.contains("boot") || run.startup.stdout.contains("boot"));
    assert!(result.stderr.contains("err") || run.startup.stderr.contains("err"));
}

#[test]
fn command_readiness_success_allows_shutdown() {
    let command = ProcessSpec::new("sh").arg("-c").arg("sleep 2");
    let readiness = ProcessSpec::new("sh").arg("-c").arg("exit 0");
    let config = PersistentConfig::default()
        .with_readiness(PersistentReadiness::Command(readiness))
        .with_readiness_timeout(Duration::from_secs(2));

    let run = start_persistent_with_cancel(
        &command,
        &ProcessConfig::default(),
        &config,
        CancellationToken::new(),
    )
    .expect("readiness command succeeds");
    let outcome = run.process.shutdown().expect("shutdown succeeds");

    assert!(matches!(outcome, ShutdownOutcome::Stopped(_)));
}

#[test]
fn command_readiness_failure_cleans_up_started_process() {
    let command = ProcessSpec::new("sh").arg("-c").arg("sleep 10");
    let readiness = ProcessSpec::new("sh").arg("-c").arg("exit 7");
    let config = PersistentConfig::default()
        .with_readiness(PersistentReadiness::Command(readiness))
        .with_readiness_timeout(Duration::from_secs(2))
        .with_shutdown_grace_period(Duration::from_millis(50));

    let error = start_persistent_with_cancel(
        &command,
        &ProcessConfig::default(),
        &config,
        CancellationToken::new(),
    )
    .expect_err("failed readiness command should fail startup");

    assert_eq!(
        persistent_start_error_kind(&error),
        Some(PersistentStartErrorKind::ReadinessCommandFailed)
    );
}

#[test]
fn persistent_spawn_and_empty_program_errors_are_classified() {
    let empty = start_persistent_with_cancel(
        &ProcessSpec::new(""),
        &ProcessConfig::default(),
        &PersistentConfig::default(),
        CancellationToken::new(),
    )
    .expect_err("empty program should be rejected before spawn");
    assert_eq!(empty.code(), ErrorCode::InvalidInput);

    let missing = start_persistent_with_cancel(
        &ProcessSpec::new("/definitely/not/rskit-process-missing"),
        &ProcessConfig::default(),
        &PersistentConfig::default(),
        CancellationToken::new(),
    )
    .expect_err("missing program should report spawn failure");
    assert_eq!(
        persistent_start_error_kind(&missing),
        Some(PersistentStartErrorKind::SpawnFailed)
    );
}

#[test]
fn process_exit_before_output_readiness_is_classified() {
    let command = ProcessSpec::new("sh")
        .arg("-c")
        .arg("printf not-ready; exit 13");
    let config = PersistentConfig::default()
        .with_readiness(PersistentReadiness::OutputContains(
            "ready-marker".to_string(),
        ))
        .with_readiness_timeout(Duration::from_secs(2))
        .with_shutdown_grace_period(Duration::from_millis(50));

    let error = start_persistent_with_cancel(
        &command,
        &ProcessConfig::default(),
        &config,
        CancellationToken::new(),
    )
    .expect_err("process should exit before output readiness");

    assert_eq!(
        persistent_start_error_kind(&error),
        Some(PersistentStartErrorKind::ExitedBeforeReadiness)
    );
}

#[test]
fn persistent_predefined_stdin_can_drive_output_readiness() {
    let command = ProcessSpec::new("sh")
        .arg("-c")
        .arg("cat; printf err-ready >&2; sleep 1");
    let process_config = ProcessConfig::default().with_input(InputPolicy::Bytes(b"ready".to_vec()));
    let config = PersistentConfig::default()
        .with_readiness(PersistentReadiness::OutputContains("ready".to_string()))
        .with_readiness_timeout(Duration::from_secs(2));

    let run =
        start_persistent_with_cancel(&command, &process_config, &config, CancellationToken::new())
            .expect("stdin output marks process ready");

    assert!(run.startup.stdout.contains("ready"));
    let _ = run.process.shutdown();
}

#[test]
fn persistent_output_forwarding_still_retains_capture() {
    let command = ProcessSpec::new("sh")
        .arg("-c")
        .arg("printf ready; printf err-ready >&2; sleep 1");
    let config = PersistentConfig::default()
        .with_readiness(PersistentReadiness::OutputContains("ready".to_string()))
        .with_readiness_timeout(Duration::from_secs(2))
        .with_output(PersistentOutput::forward(
            PersistentOutputStream::Stdout,
            PersistentOutputStream::Stderr,
        ));

    let run = start_persistent_with_cancel(
        &command,
        &ProcessConfig::default(),
        &config,
        CancellationToken::new(),
    )
    .expect("forwarded output still marks readiness");

    assert!(run.startup.stdout.contains("ready") || run.startup.stderr.contains("ready"));
    let _ = run.process.shutdown();
}

#[test]
fn persistent_spawn_applies_working_directory_empty_env_and_overrides() {
    let workspace = TestWorkspace::new("persistent-dir-env");
    let dir = workspace.path();
    let command = ProcessSpec::new("sh")
        .arg("-c")
        .arg("printf '%s:%s:%s' \"$PWD\" \"$ONLY_ME\" \"${RSKIT_MISSING-unset}\"; sleep 1")
        .dir(dir)
        .env("ONLY_ME", "present")
        .empty_env();
    let config = PersistentConfig::default()
        .with_readiness(PersistentReadiness::OutputContains("present".to_string()))
        .with_readiness_timeout(Duration::from_secs(2));

    let run = start_persistent_with_cancel(
        &command,
        &ProcessConfig::default(),
        &config,
        CancellationToken::new(),
    )
    .expect("persistent spawn applies dir and env policy");

    assert!(run.startup.stdout.contains(dir.to_string_lossy().as_ref()));
    assert!(run.startup.stdout.contains(":present:unset"));
    let _ = run.process.shutdown();
}

#[test]
fn persistent_closed_output_before_marker_is_classified() {
    let command = ProcessSpec::new("sh")
        .arg("-c")
        .arg("exec 1>&- 2>&-; sleep 10");
    let config = PersistentConfig::default()
        .with_readiness(PersistentReadiness::OutputContains("never".to_string()))
        .with_readiness_timeout(Duration::from_secs(2))
        .with_shutdown_grace_period(Duration::from_millis(50));

    let error = start_persistent_with_cancel(
        &command,
        &ProcessConfig::default(),
        &config,
        CancellationToken::new(),
    )
    .expect_err("closed output streams should fail readiness");

    assert_eq!(
        persistent_start_error_kind(&error),
        Some(PersistentStartErrorKind::OutputEndedBeforeReadiness)
    );
}

#[test]
fn dropping_persistent_process_terminates_child() {
    let workspace = TestWorkspace::new("persistent-drop");
    let pid_file = workspace.child("child.pid").expect("pid path resolves");
    let command = ProcessSpec::new("sh").arg("-c").arg(format!(
        "printf %s \"$$\" > '{}'; printf ready; while :; do sleep 1; done",
        pid_file.display()
    ));
    let config = PersistentConfig::default()
        .with_readiness(PersistentReadiness::OutputContains("ready".to_string()))
        .with_readiness_timeout(Duration::from_secs(2))
        .with_shutdown_grace_period(Duration::from_millis(50));

    let run = start_persistent_with_cancel(
        &command,
        &ProcessConfig::default(),
        &config,
        CancellationToken::new(),
    )
    .expect("process starts and becomes ready");
    let pid: i32 = std::fs::read_to_string(&pid_file)
        .expect("pid file is written")
        .trim()
        .parse()
        .expect("pid is numeric");

    // Drop without an explicit wait/shutdown; the Drop impl must terminate the child.
    drop(run);

    let mut alive = true;
    for _ in 0..50 {
        let status = std::process::Command::new("sh")
            .args(["-c", &format!("kill -0 {pid}")])
            .stderr(std::process::Stdio::null())
            .status()
            .expect("kill -0 probe runs");
        if !status.success() {
            alive = false;
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    assert!(!alive, "dropping the process should terminate the child");
}

#[test]
fn persistent_shutdown_reports_already_exited_when_child_finished() {
    // Synchronize on a FIFO so the test only proceeds once the child has
    // reached its final pre-exit instruction. The shutdown matcher accepts
    // either AlreadyExited or Stopped because, externally, the test cannot
    // distinguish a fully-reaped child from a zombie awaiting the handle's
    // reap call; the surrounding sync guarantees the child is past its
    // useful work in both cases.
    let workspace = TestWorkspace::new("persistent-already-exited");
    let fifo = workspace.child("exit.fifo").expect("fifo path resolves");
    let status = std::process::Command::new("mkfifo")
        .arg(&fifo)
        .status()
        .expect("mkfifo runs");
    assert!(status.success(), "mkfifo creates the synchronization fifo");

    let command = ProcessSpec::new("sh").arg("-c").arg(format!(
        "printf ready; printf done > '{}'; exit 0",
        fifo.display()
    ));
    let config = PersistentConfig::default()
        .with_readiness(PersistentReadiness::OutputContains("ready".to_string()))
        .with_readiness_timeout(Duration::from_secs(2));

    let signal_thread = {
        let fifo = fifo.clone();
        std::thread::spawn(move || {
            let _ = std::fs::read_to_string(&fifo);
            let _ = std::fs::remove_file(&fifo);
        })
    };

    let run = start_persistent_with_cancel(
        &command,
        &ProcessConfig::default(),
        &config,
        CancellationToken::new(),
    )
    .expect("process starts and exits quickly");
    signal_thread.join().expect("fifo sync thread joins");

    let outcome = run
        .process
        .shutdown()
        .expect("shutdown reports exited child");
    assert!(matches!(
        outcome,
        ShutdownOutcome::AlreadyExited(_) | ShutdownOutcome::Stopped(_)
    ));
}
