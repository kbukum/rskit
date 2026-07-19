use std::{
    io::Read,
    path::{Path, PathBuf},
    sync::atomic::{AtomicUsize, Ordering},
    time::Duration,
};

use tokio_util::sync::CancellationToken;

use super::{PersistentConfig, PersistentReadiness, ShutdownOutcome, start_persistent_with_cancel};
#[cfg(unix)]
use crate::pty::PtyIo;
use crate::{
    ErrorCode, InheritedIo, InputPolicy, ObservedIo, OutputObserver, ProcessConfig, ProcessIo,
    ProcessSpec, SignalPolicy,
};

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

fn process_is_alive(pid: i32) -> bool {
    // SAFETY:
    // `kill(pid, 0)` only checks signalability for a pid captured from the child process itself.
    // ESRCH means the process no longer exists.
    unsafe { libc::kill(pid, 0) == 0 }
}

fn kill_process(pid: i32) {
    // SAFETY: The pid is read from a test-owned child process
    // and cleaned up after verifying non-descendant shutdown behavior.
    unsafe {
        let _ = libc::kill(pid, libc::SIGKILL);
    }
}

fn descendant_pid_file(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!(
        "rskit-process-{name}-{}-{}",
        std::process::id(),
        FIFO_ID.fetch_add(1, Ordering::SeqCst)
    ))
}

fn descendant_command(pid_file: &Path) -> ProcessSpec {
    ProcessSpec::new("sh").arg("-c").arg(format!(
        "trap '' HUP TERM; nohup sh -c 'trap \"\" HUP TERM; sleep 20' >/dev/null 2>&1 & printf %s $! > {}; printf ready; while :; do sleep 1; done",
        shell_path(pid_file)
    ))
}

fn read_descendant_pid(pid_file: &Path) -> i32 {
    std::fs::read_to_string(pid_file)
        .expect("pid file is readable")
        .parse()
        .expect("pid is valid")
}

fn assert_descendant_survived_shutdown(descendant_pid: i32, pid_file: PathBuf) {
    assert!(process_is_alive(descendant_pid));
    kill_process(descendant_pid);
    let _ = std::fs::remove_file(pid_file);
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
    let command = ProcessSpec::new("sh")
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
    let command = ProcessSpec::new("sh")
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
    let command = ProcessSpec::new("sh").arg("-c").arg(format!(
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
    let command = ProcessSpec::new("sh")
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
    let command = ProcessSpec::new("sh")
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
    let command = ProcessSpec::new("sh").arg("-c").arg("sleep 10");
    let config = PersistentConfig::default();
    let cancel = CancellationToken::new();
    cancel.cancel();

    let error = start_persistent_with_cancel(&command, &ProcessConfig::default(), &config, cancel)
        .expect_err("pre-cancelled startup should fail before spawn");

    assert_eq!(error.code(), ErrorCode::Cancelled);
}

#[test]
fn cancellation_during_startup_returns_cancelled() {
    let fifo = create_fifo("startup-cancel");
    let command = ProcessSpec::new("sh")
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

    assert_eq!(error.code(), ErrorCode::Cancelled);
}

#[test]
fn cancellation_during_command_readiness_returns_promptly() {
    let fifo = create_fifo("command-readiness-cancel");
    let command = ProcessSpec::new("sh").arg("-c").arg("sleep 10");
    let readiness = ProcessSpec::new("sh").arg("-c").arg(format!(
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

    assert_eq!(error.code(), ErrorCode::Cancelled);
    assert!(start.elapsed() < Duration::from_secs(2));
}

#[test]
fn command_readiness_can_start_inside_tokio_runtime() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime starts");

    runtime.block_on(async {
        let command = ProcessSpec::new("sh").arg("-c").arg("sleep 1");
        let readiness = ProcessSpec::new("sh").arg("-c").arg("true");
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
    let command = ProcessSpec::new("sh").arg("-c").arg("sleep 10");
    let readiness = ProcessSpec::new("sh")
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

    assert_eq!(error.code(), ErrorCode::Timeout);
}

#[test]
fn empty_output_matcher_is_invalid() {
    let command = ProcessSpec::new("sh").arg("-c").arg("sleep 10");
    let config = PersistentConfig::default()
        .with_readiness(PersistentReadiness::OutputContains(String::new()));

    let error = start_persistent_with_cancel(
        &command,
        &ProcessConfig::default(),
        &config,
        CancellationToken::new(),
    )
    .expect_err("empty output matcher should be rejected before spawn");

    assert_eq!(error.code(), ErrorCode::InvalidInput);
}

#[test]
fn persistent_rejects_inherited_io_mode() {
    let command = ProcessSpec::new("sh").arg("-c").arg("sleep 10");
    let process_config = ProcessConfig::default().with_io(ProcessIo::inherited(InheritedIo::new()));

    let error = start_persistent_with_cancel(
        &command,
        &process_config,
        &PersistentConfig::default(),
        CancellationToken::new(),
    )
    .expect_err("persistent startup should reject inherited mode");

    assert_eq!(error.code(), ErrorCode::InvalidInput);
}

#[test]
fn persistent_rejects_observed_io_mode() {
    let command = ProcessSpec::new("sh").arg("-c").arg("sleep 10");
    let process_config = ProcessConfig::default()
        .with_io(ProcessIo::observed(ObservedIo::new(OutputObserver::new())));

    let error = start_persistent_with_cancel(
        &command,
        &process_config,
        &PersistentConfig::default(),
        CancellationToken::new(),
    )
    .expect_err("persistent startup should reject observed mode");

    assert_eq!(error.code(), ErrorCode::InvalidInput);
}

#[cfg(unix)]
#[test]
fn persistent_rejects_pty_io_mode() {
    let command = ProcessSpec::new("sh").arg("-c").arg("sleep 10");
    let process_config = ProcessConfig::default().with_io(ProcessIo::pty(PtyIo::default()));

    let error = start_persistent_with_cancel(
        &command,
        &process_config,
        &PersistentConfig::default(),
        CancellationToken::new(),
    )
    .expect_err("persistent startup should reject pty mode");

    assert_eq!(error.code(), ErrorCode::InvalidInput);
}

#[test]
fn persistent_rejects_inherited_stdin_for_captured_mode() {
    let command = ProcessSpec::new("sh").arg("-c").arg("sleep 10");
    let process_config = ProcessConfig::default().with_input(InputPolicy::Inherit);

    let error = start_persistent_with_cancel(
        &command,
        &process_config,
        &PersistentConfig::default(),
        CancellationToken::new(),
    )
    .expect_err("persistent startup should reject live stdin");

    assert_eq!(error.code(), ErrorCode::InvalidInput);
}

#[test]
fn persistent_shutdown_can_leave_descendants_running_when_configured() {
    let pid_file = descendant_pid_file("descendant-shutdown");
    let command = descendant_command(&pid_file);
    let signal = SignalPolicy::default().with_terminate_descendants(false);
    let process_config = ProcessConfig::default().with_signal_policy(signal);
    let config = PersistentConfig::default()
        .with_readiness(PersistentReadiness::OutputContains("ready".to_string()))
        .with_shutdown_grace_period(Duration::from_millis(50));

    let run =
        start_persistent_with_cancel(&command, &process_config, &config, CancellationToken::new())
            .expect("process starts");
    let descendant_pid = read_descendant_pid(&pid_file);

    let _ = run.process.shutdown().expect("shutdown succeeds");

    assert_descendant_survived_shutdown(descendant_pid, pid_file);
}

#[test]
fn persistent_shutdown_without_process_group_leaves_descendants_running() {
    let pid_file = descendant_pid_file("descendant-no-process-group");
    let command = descendant_command(&pid_file);
    let signal = SignalPolicy::default().with_create_process_group(false);
    let process_config = ProcessConfig::default().with_signal_policy(signal);
    let config = PersistentConfig::default()
        .with_readiness(PersistentReadiness::OutputContains("ready".to_string()))
        .with_shutdown_grace_period(Duration::from_millis(50));

    let run =
        start_persistent_with_cancel(&command, &process_config, &config, CancellationToken::new())
            .expect("process starts");
    let descendant_pid = read_descendant_pid(&pid_file);

    let _ = run.process.shutdown().expect("shutdown succeeds");

    assert_descendant_survived_shutdown(descendant_pid, pid_file);
}

#[test]
fn persistent_wait_cancel_can_leave_descendants_running_when_configured() {
    let pid_file = descendant_pid_file("descendant-wait-cancel");
    let command = descendant_command(&pid_file);
    let signal = SignalPolicy::default().with_terminate_descendants(false);
    let process_config = ProcessConfig::default().with_signal_policy(signal);
    let config = PersistentConfig::default()
        .with_readiness(PersistentReadiness::OutputContains("ready".to_string()))
        .with_shutdown_grace_period(Duration::from_millis(50));
    let cancel = CancellationToken::new();

    let run = start_persistent_with_cancel(&command, &process_config, &config, cancel.clone())
        .expect("process starts");
    let descendant_pid = read_descendant_pid(&pid_file);
    cancel.cancel();
    let _ = run.process.wait().expect("wait observes cancellation");

    assert_descendant_survived_shutdown(descendant_pid, pid_file);
}

#[test]
fn persistent_start_cleanup_can_leave_descendants_running_when_configured() {
    let pid_file = descendant_pid_file("descendant-start-cleanup");
    let command = descendant_command(&pid_file);
    let signal = SignalPolicy::default().with_terminate_descendants(false);
    let process_config = ProcessConfig::default().with_signal_policy(signal);
    let config = PersistentConfig::default()
        .with_readiness(PersistentReadiness::OutputContains(
            "never-ready".to_string(),
        ))
        .with_readiness_timeout(Duration::from_millis(50))
        .with_shutdown_grace_period(Duration::from_millis(50));

    let error =
        start_persistent_with_cancel(&command, &process_config, &config, CancellationToken::new())
            .expect_err("startup should fail readiness");
    let descendant_pid = read_descendant_pid(&pid_file);

    assert_eq!(error.code(), ErrorCode::Timeout);
    assert_descendant_survived_shutdown(descendant_pid, pid_file);
}

#[test]
fn stdin_writer_does_not_block_output_readiness() {
    let stdin = vec![b'x'; 2 * 1024 * 1024];
    let command = ProcessSpec::new("sh").arg("-c").arg(
        "dd if=/dev/zero bs=1024 count=2048 2>/dev/null; \
             cat >/dev/null; printf ready; sleep 1",
    );
    let process_config = ProcessConfig::default().with_input(InputPolicy::Bytes(stdin));
    let config = PersistentConfig::default()
        .with_readiness(PersistentReadiness::OutputContains("ready".to_string()))
        .with_readiness_timeout(Duration::from_secs(2))
        .with_max_capture_bytes(1024);

    let run =
        start_persistent_with_cancel(&command, &process_config, &config, CancellationToken::new())
            .expect("output is drained while stdin is written");

    assert!(run.startup.stdout_truncated);
    let _ = run.process.shutdown();
}
