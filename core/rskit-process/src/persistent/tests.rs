use std::{
    io::Read,
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
    time::Duration,
};

use tokio_util::sync::CancellationToken;

use super::{PersistentConfig, PersistentReadiness, ShutdownOutcome, start_persistent_with_cancel};
use crate::{Command, ErrorCode, ProcessConfig};

static FIFO_ID: AtomicUsize = AtomicUsize::new(0);

fn create_fifo(name: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "rskit-process-{name}-{}-{}",
        std::process::id(),
        FIFO_ID.fetch_add(1, Ordering::SeqCst)
    ));
    let status = std::process::Command::new("mkfifo")
        .arg(&path)
        .status()
        .expect("mkfifo command runs");
    assert!(status.success(), "mkfifo command succeeds");
    path
}

fn shell_path(path: &Path) -> String {
    format!("'{}'", path.display().to_string().replace('\'', "'\\''"))
}

fn wait_for_fifo_signal(path: PathBuf) {
    let mut file = std::fs::File::open(&path).expect("fifo opens for reading");
    let mut signal = String::new();
    file.read_to_string(&mut signal)
        .expect("fifo signal is read");
    let _ = std::fs::remove_file(path);
}

fn wait_for_fifo_byte(path: PathBuf) {
    let mut file = std::fs::File::open(&path).expect("fifo opens for reading");
    let mut signal = [0_u8; 1];
    file.read_exact(&mut signal).expect("fifo signal is read");
    let _ = std::fs::remove_file(path);
}

fn cancel_after_fifo_signal(
    path: PathBuf,
    cancel: CancellationToken,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        wait_for_fifo_byte(path);
        cancel.cancel();
    })
}

#[test]
fn output_matcher_marks_persistent_process_ready() {
    let command = Command::new("sh")
        .arg("-c")
        .arg("printf listening; sleep 2");
    let config = PersistentConfig::default()
        .with_readiness(PersistentReadiness::OutputContains("listening".to_string()))
        .with_readiness_timeout(Duration::from_secs(2));

    let run = start_persistent_with_cancel(
        &command,
        &ProcessConfig::default(),
        &config,
        CancellationToken::new(),
    )
    .expect("process becomes ready");

    assert!(run.startup.stdout.contains("listening"));
    let outcome = run.process.shutdown().expect("shutdown succeeds");
    assert!(matches!(outcome, ShutdownOutcome::Stopped(_)));
}

#[test]
fn output_matcher_spans_multiple_reads() {
    let command = Command::new("sh")
        .arg("-c")
        .arg("printf lis; sleep 0.05; printf tening; sleep 2");
    let config = PersistentConfig::default()
        .with_readiness(PersistentReadiness::OutputContains("listening".to_string()))
        .with_readiness_timeout(Duration::from_secs(2));

    let run = start_persistent_with_cancel(
        &command,
        &ProcessConfig::default(),
        &config,
        CancellationToken::new(),
    )
    .expect("split marker is matched");

    assert!(run.startup.stdout.contains("listening"));
    let _ = run.process.shutdown();
}

#[test]
fn reports_already_exited_on_shutdown() {
    let fifo = create_fifo("already-exited");
    let signal_thread = std::thread::spawn({
        let fifo = fifo.clone();
        move || wait_for_fifo_signal(fifo)
    });
    let command = Command::new("sh").arg("-c").arg(format!(
        "printf listening; printf done > {}; exit 0",
        shell_path(&fifo)
    ));
    let config = PersistentConfig::default()
        .with_readiness(PersistentReadiness::OutputContains("listening".to_string()))
        .with_readiness_timeout(Duration::from_secs(2));

    let run = start_persistent_with_cancel(
        &command,
        &ProcessConfig::default(),
        &config,
        CancellationToken::new(),
    )
    .expect("process starts");
    signal_thread.join().expect("signal thread joins");

    let outcome = run.process.shutdown().expect("shutdown reports outcome");
    assert!(matches!(
        outcome,
        ShutdownOutcome::AlreadyExited(_) | ShutdownOutcome::Stopped(_)
    ));
}

#[test]
fn cancellation_interrupts_process() {
    let command = Command::new("sh")
        .arg("-c")
        .arg("trap '' TERM INT; printf ready; while :; do sleep 1; done");
    let config = PersistentConfig::default()
        .with_readiness(PersistentReadiness::OutputContains("ready".to_string()))
        .with_readiness_timeout(Duration::from_secs(2))
        .with_shutdown_grace_period(Duration::from_millis(50));
    let cancel = CancellationToken::new();

    let run =
        start_persistent_with_cancel(&command, &ProcessConfig::default(), &config, cancel.clone())
            .expect("process starts");
    let start = std::time::Instant::now();
    cancel.cancel();

    let result = run.process.wait().expect("wait returns process result");
    assert!(result.cancelled);
    assert_ne!(result.exit_code, Some(0));
    assert!(start.elapsed() < Duration::from_secs(1));
}

#[test]
fn shutdown_preserves_cancellation_state() {
    let command = Command::new("sh")
        .arg("-c")
        .arg("trap '' TERM INT; printf ready; while :; do sleep 1; done");
    let config = PersistentConfig::default()
        .with_readiness(PersistentReadiness::OutputContains("ready".to_string()))
        .with_readiness_timeout(Duration::from_secs(2))
        .with_shutdown_grace_period(Duration::from_secs(5));
    let cancel = CancellationToken::new();

    let run =
        start_persistent_with_cancel(&command, &ProcessConfig::default(), &config, cancel.clone())
            .expect("process starts");
    cancel.cancel();

    let outcome = run.process.shutdown().expect("shutdown succeeds");
    let ShutdownOutcome::Stopped(result) = outcome else {
        panic!("shutdown should stop the still-running cancelled process");
    };
    assert!(result.cancelled);
}

#[test]
fn already_cancelled_token_does_not_spawn() {
    let command = Command::new("sh").arg("-c").arg("sleep 10");
    let config = PersistentConfig::default();
    let cancel = CancellationToken::new();
    cancel.cancel();

    let error = start_persistent_with_cancel(&command, &ProcessConfig::default(), &config, cancel)
        .expect_err("pre-cancelled startup should fail before spawn");

    assert_eq!(error.code, ErrorCode::Cancelled);
}

#[test]
fn cancellation_during_startup_returns_cancelled() {
    let fifo = create_fifo("startup-cancel");
    let command = Command::new("sh")
        .arg("-c")
        .arg(format!("printf spawned > {}; sleep 10", shell_path(&fifo)));
    let config = PersistentConfig::default()
        .with_readiness(PersistentReadiness::OutputContains("ready".to_string()))
        .with_readiness_timeout(Duration::from_secs(2))
        .with_shutdown_grace_period(Duration::from_millis(50));
    let cancel = CancellationToken::new();
    let cancel_thread = cancel_after_fifo_signal(fifo, cancel.clone());

    let error = start_persistent_with_cancel(&command, &ProcessConfig::default(), &config, cancel)
        .expect_err("startup cancellation should fail with cancelled semantics");
    cancel_thread.join().expect("cancel thread joins");

    assert_eq!(error.code, ErrorCode::Cancelled);
}

#[test]
fn cancellation_during_command_readiness_returns_promptly() {
    let fifo = create_fifo("command-readiness-cancel");
    let command = Command::new("sh").arg("-c").arg("sleep 10");
    let readiness = Command::new("sh").arg("-c").arg(format!(
        "printf readiness > {}; exec sleep 10",
        shell_path(&fifo)
    ));
    let config = PersistentConfig::default()
        .with_readiness(PersistentReadiness::Command(readiness))
        .with_readiness_timeout(Duration::from_secs(10))
        .with_shutdown_grace_period(Duration::from_millis(50));
    let cancel = CancellationToken::new();
    let cancel_thread = cancel_after_fifo_signal(fifo, cancel.clone());
    let start = std::time::Instant::now();

    let error = start_persistent_with_cancel(&command, &ProcessConfig::default(), &config, cancel)
        .expect_err("command readiness cancellation should fail promptly");
    cancel_thread.join().expect("cancel thread joins");

    assert_eq!(error.code, ErrorCode::Cancelled);
    assert!(start.elapsed() < Duration::from_secs(2));
}

#[test]
fn command_readiness_can_start_inside_tokio_runtime() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime starts");

    runtime.block_on(async {
        let command = Command::new("sh").arg("-c").arg("sleep 1");
        let readiness = Command::new("sh").arg("-c").arg("true");
        let config = PersistentConfig::default()
            .with_readiness(PersistentReadiness::Command(readiness))
            .with_readiness_timeout(Duration::from_secs(2));

        let run = start_persistent_with_cancel(
            &command,
            &ProcessConfig::default(),
            &config,
            CancellationToken::new(),
        )
        .expect("persistent startup should not nest runtimes");

        let _ = run.process.shutdown();
    });
}

#[test]
fn timed_out_readiness_command_is_not_accepted_after_successful_sigterm() {
    let command = Command::new("sh").arg("-c").arg("sleep 10");
    let readiness = Command::new("sh")
        .arg("-c")
        .arg("trap 'exit 0' TERM; sleep 10");
    let config = PersistentConfig::default()
        .with_readiness(PersistentReadiness::Command(readiness))
        .with_readiness_timeout(Duration::from_millis(20))
        .with_shutdown_grace_period(Duration::from_millis(50));

    let error = start_persistent_with_cancel(
        &command,
        &ProcessConfig::default(),
        &config,
        CancellationToken::new(),
    )
    .expect_err("timed out readiness command should not be accepted");

    assert_eq!(error.code, ErrorCode::Timeout);
}

#[test]
fn empty_output_matcher_is_invalid() {
    let command = Command::new("sh").arg("-c").arg("sleep 10");
    let config = PersistentConfig::default()
        .with_readiness(PersistentReadiness::OutputContains(String::new()));

    let error = start_persistent_with_cancel(
        &command,
        &ProcessConfig::default(),
        &config,
        CancellationToken::new(),
    )
    .expect_err("empty output matcher should be rejected before spawn");

    assert_eq!(error.code, ErrorCode::InvalidInput);
}

#[test]
fn stdin_writer_does_not_block_output_readiness() {
    let stdin = vec![b'x'; 2 * 1024 * 1024];
    let command = Command::new("sh")
        .arg("-c")
        .arg(
            "dd if=/dev/zero bs=1024 count=2048 2>/dev/null; \
             cat >/dev/null; printf ready; sleep 1",
        )
        .stdin(stdin);
    let config = PersistentConfig::default()
        .with_readiness(PersistentReadiness::OutputContains("ready".to_string()))
        .with_readiness_timeout(Duration::from_secs(2))
        .with_max_capture_bytes(1024);

    let run = start_persistent_with_cancel(
        &command,
        &ProcessConfig::default(),
        &config,
        CancellationToken::new(),
    )
    .expect("output is drained while stdin is written");

    assert!(run.startup.stdout_truncated);
    let _ = run.process.shutdown();
}
