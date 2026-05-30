//! Persistent subprocess lifecycle support.

use std::{
    io::{ErrorKind, Read, Write},
    process::{Child, ChildStdin, Command as StdCommand, ExitStatus, Stdio},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

use parking_lot::Mutex;
use tokio_util::sync::CancellationToken;

use crate::{
    AppError, AppResult, Command, ErrorCode, ProcessConfig, ProcessResult,
    command::DEFAULT_MAX_OUTPUT_BYTES,
    process_group::{isolate, kill, terminate},
    runner,
};

type Capture = Arc<Mutex<CapturedOutput>>;
type ReaderThread = Option<thread::JoinHandle<AppResult<()>>>;
type StdinThread = Option<thread::JoinHandle<AppResult<()>>>;

/// Persistent process readiness policy.
#[derive(Debug, Clone, Eq, PartialEq)]
#[non_exhaustive]
pub enum PersistentReadiness {
    /// The process is ready immediately after it is spawned.
    Started,
    /// The process is ready when either output stream contains the text.
    OutputContains(String),
    /// The process is ready when a command exits successfully.
    Command(Command),
}

/// Configuration for a persistent process.
#[derive(Debug, Clone)]
pub struct PersistentConfig {
    /// Readiness policy used before returning the running process.
    pub readiness: PersistentReadiness,
    /// Maximum time to wait for readiness.
    pub readiness_timeout: Duration,
    /// Maximum time to wait after a graceful shutdown request before killing.
    pub shutdown_grace_period: Duration,
    /// Maximum retained bytes for each captured output stream.
    pub max_capture_bytes: Option<usize>,
    /// Output forwarding policy.
    pub output: PersistentOutput,
}

impl Default for PersistentConfig {
    fn default() -> Self {
        Self {
            readiness: PersistentReadiness::Started,
            readiness_timeout: Duration::from_secs(30),
            shutdown_grace_period: Duration::from_secs(5),
            max_capture_bytes: Some(DEFAULT_MAX_OUTPUT_BYTES),
            output: PersistentOutput::capture_only(),
        }
    }
}

impl PersistentConfig {
    /// Set the readiness policy.
    #[must_use]
    pub fn with_readiness(mut self, readiness: PersistentReadiness) -> Self {
        self.readiness = readiness;
        self
    }

    /// Set the readiness timeout.
    #[must_use]
    pub fn with_readiness_timeout(mut self, timeout: Duration) -> Self {
        self.readiness_timeout = timeout;
        self
    }

    /// Set the shutdown grace period.
    #[must_use]
    pub fn with_shutdown_grace_period(mut self, grace_period: Duration) -> Self {
        self.shutdown_grace_period = grace_period;
        self
    }

    /// Set the maximum retained bytes for each output stream.
    #[must_use]
    pub fn with_max_capture_bytes(mut self, bytes: usize) -> Self {
        self.max_capture_bytes = Some(bytes);
        self
    }

    /// Disable capture bounds.
    #[must_use]
    pub fn with_unbounded_capture(mut self) -> Self {
        self.max_capture_bytes = None;
        self
    }

    /// Set the output forwarding policy.
    #[must_use]
    pub fn with_output(mut self, output: PersistentOutput) -> Self {
        self.output = output;
        self
    }
}

/// Persistent process output forwarding policy.
#[derive(Debug, Clone, Copy)]
pub struct PersistentOutput {
    stdout: Option<PersistentOutputStream>,
    stderr: Option<PersistentOutputStream>,
}

impl PersistentOutput {
    /// Capture output without forwarding it to the parent process streams.
    #[must_use]
    pub const fn capture_only() -> Self {
        Self {
            stdout: None,
            stderr: None,
        }
    }

    /// Capture and forward stdout/stderr to the selected parent streams.
    #[must_use]
    pub const fn forward(stdout: PersistentOutputStream, stderr: PersistentOutputStream) -> Self {
        Self {
            stdout: Some(stdout),
            stderr: Some(stderr),
        }
    }

    const fn stdout_stream(self) -> Option<PersistentOutputStream> {
        self.stdout
    }

    const fn stderr_stream(self) -> Option<PersistentOutputStream> {
        self.stderr
    }
}

/// Parent stream used for forwarding persistent output.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
#[non_exhaustive]
pub enum PersistentOutputStream {
    /// Forward bytes to parent stdout.
    Stdout,
    /// Forward bytes to parent stderr.
    Stderr,
}

/// Captured output retained while waiting for persistent readiness.
#[derive(Debug, Clone)]
pub struct PersistentStartup {
    /// Captured stdout at the moment readiness completed.
    pub stdout: String,
    /// Captured stdout bytes at the moment readiness completed.
    pub stdout_bytes: Vec<u8>,
    /// Captured stderr at the moment readiness completed.
    pub stderr: String,
    /// Captured stderr bytes at the moment readiness completed.
    pub stderr_bytes: Vec<u8>,
    /// Whether stdout capture exceeded the configured limit before readiness.
    pub stdout_truncated: bool,
    /// Whether stderr capture exceeded the configured limit before readiness.
    pub stderr_truncated: bool,
    /// Time elapsed from spawn until readiness completed.
    pub duration: Duration,
}

/// Result of starting a persistent process.
#[derive(Debug)]
pub struct PersistentRun {
    /// Startup output captured while waiting for readiness.
    pub startup: PersistentStartup,
    /// Running persistent process handle.
    pub process: PersistentProcess,
}

/// Outcome of requesting persistent process shutdown.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum ShutdownOutcome {
    /// The process had already exited before shutdown was requested.
    AlreadyExited(ProcessResult),
    /// The process was stopped by the shutdown request.
    Stopped(ProcessResult),
}

/// Running persistent process.
#[derive(Debug)]
pub struct PersistentProcess {
    child: Child,
    stdin_thread: StdinThread,
    stdout_thread: ReaderThread,
    stderr_thread: ReaderThread,
    cancel_thread: Option<CancelThread>,
    cancelled: Arc<AtomicBool>,
    stdout: Capture,
    stderr: Capture,
    start: Instant,
    shutdown_grace_period: Duration,
    stopped: bool,
}

impl PersistentProcess {
    /// Wait for the persistent process to exit naturally.
    pub fn wait(mut self) -> AppResult<ProcessResult> {
        self.wait_inner()
    }

    /// Gracefully stop the persistent process.
    pub fn shutdown(mut self) -> AppResult<ShutdownOutcome> {
        self.shutdown_inner()
    }

    fn wait_inner(&mut self) -> AppResult<ProcessResult> {
        if self.stopped {
            return Err(AppError::new(
                ErrorCode::Conflict,
                "persistent process already stopped",
            ));
        }
        let status = wait_for_exit(
            &mut self.child,
            &self.cancel_thread,
            self.shutdown_grace_period,
        )?;
        self.stopped = true;
        let cancelled = self.cancelled.load(Ordering::SeqCst);
        self.stop_cancel_thread()?;
        self.completed_result(status, false, cancelled)
    }

    fn shutdown_inner(&mut self) -> AppResult<ShutdownOutcome> {
        if self.stopped {
            return Err(AppError::new(
                ErrorCode::Conflict,
                "persistent process already stopped",
            ));
        }
        if let Some(status) = self.child.try_wait().map_err(AppError::internal)? {
            self.stopped = true;
            self.stop_cancel_thread()?;
            let cancelled = self.cancelled.load(Ordering::SeqCst);
            return self
                .completed_result(status, false, cancelled)
                .map(ShutdownOutcome::AlreadyExited);
        }

        self.stop_cancel_thread()?;
        let pid = self.child.id();
        if !terminate(pid) {
            let _ = self.child.kill();
        }
        let status = wait_for_shutdown(&mut self.child, pid, self.shutdown_grace_period)?;
        self.stopped = true;
        let cancelled = self.cancelled.load(Ordering::SeqCst);
        self.completed_result(status, false, cancelled)
            .map(ShutdownOutcome::Stopped)
    }

    fn completed_result(
        &mut self,
        status: ExitStatus,
        timed_out: bool,
        cancelled: bool,
    ) -> AppResult<ProcessResult> {
        join_stdin(self.stdin_thread.take())?;
        join_reader(self.stdout_thread.take())?;
        join_reader(self.stderr_thread.take())?;
        let stdout = take_capture(&self.stdout);
        let stderr = take_capture(&self.stderr);
        Ok(ProcessResult {
            exit_code: status.code(),
            stdout: String::from_utf8_lossy(&stdout.bytes).into_owned(),
            stdout_bytes: stdout.bytes,
            stderr: String::from_utf8_lossy(&stderr.bytes).into_owned(),
            stderr_bytes: stderr.bytes,
            stdout_truncated: stdout.truncated,
            stderr_truncated: stderr.truncated,
            duration: self.start.elapsed(),
            timed_out,
            cancelled,
        })
    }

    fn stop_cancel_thread(&mut self) -> AppResult<()> {
        if let Some(thread) = self.cancel_thread.take() {
            thread.stop()
        } else {
            Ok(())
        }
    }
}

impl Drop for PersistentProcess {
    fn drop(&mut self) {
        if !self.stopped {
            let _ = self.shutdown_inner();
        }
    }
}

/// Start a persistent process and wait for its readiness policy.
pub fn start_persistent_with_cancel(
    command: &Command,
    process_config: &ProcessConfig,
    persistent_config: &PersistentConfig,
    cancel: CancellationToken,
) -> AppResult<PersistentRun> {
    if command.program.as_os_str().is_empty() {
        return Err(AppError::invalid_input("program", "must not be empty"));
    }
    if cancel.is_cancelled() {
        return Err(AppError::cancelled("persistent process startup"));
    }
    validate_readiness(&persistent_config.readiness)?;

    let start = Instant::now();
    let mut child = spawn_child(command, process_config)?;
    let stdout = Arc::new(Mutex::new(CapturedOutput::default()));
    let stderr = Arc::new(Mutex::new(CapturedOutput::default()));
    let cancelled = Arc::new(AtomicBool::new(false));
    let cancel_thread = match spawn_cancel_thread(
        child.id(),
        cancel.clone(),
        Arc::clone(&cancelled),
        persistent_config.shutdown_grace_period,
    ) {
        Ok(thread) => Some(thread),
        Err(error) => {
            let _ = cleanup_spawned_child(&mut child, persistent_config.shutdown_grace_period);
            return Err(error);
        }
    };
    let (ready_tx, ready_rx) = mpsc::channel();
    let (stdout_thread, stderr_thread) = spawn_output_readers(
        &mut child,
        &stdout,
        &stderr,
        &ready_tx,
        &persistent_config.readiness,
        persistent_config.output,
        persistent_config.max_capture_bytes,
    );
    let stdin_thread = spawn_stdin_writer(&mut child, command.stdin.clone());

    match &persistent_config.readiness {
        PersistentReadiness::Started => {
            let _ = ready_tx.send(());
        }
        PersistentReadiness::Command(command) => {
            if let Err(error) = run_readiness_command(
                command,
                process_config,
                persistent_config.readiness_timeout,
                cancel.clone(),
            ) {
                let mut process = persistent_process(
                    child,
                    stdin_thread,
                    stdout_thread,
                    stderr_thread,
                    cancel_thread,
                    cancelled,
                    stdout,
                    stderr,
                    start,
                    persistent_config,
                );
                let _ = process.shutdown_inner();
                return Err(error);
            }
            let _ = ready_tx.send(());
        }
        PersistentReadiness::OutputContains(_) => {}
    }
    drop(ready_tx);

    if let Err(error) = ready_rx.recv_timeout(persistent_config.readiness_timeout) {
        let readiness_error = readiness_wait_error(&mut child, error, &cancelled)?;
        let mut process = persistent_process(
            child,
            stdin_thread,
            stdout_thread,
            stderr_thread,
            cancel_thread,
            cancelled,
            stdout,
            stderr,
            start,
            persistent_config,
        );
        let _ = process.shutdown_inner();
        return Err(readiness_error);
    }

    let stdout_startup = take_capture(&stdout);
    let stderr_startup = take_capture(&stderr);
    let process = persistent_process(
        child,
        stdin_thread,
        stdout_thread,
        stderr_thread,
        cancel_thread,
        cancelled,
        stdout,
        stderr,
        start,
        persistent_config,
    );

    Ok(PersistentRun {
        startup: PersistentStartup {
            stdout: String::from_utf8_lossy(&stdout_startup.bytes).into_owned(),
            stdout_bytes: stdout_startup.bytes,
            stderr: String::from_utf8_lossy(&stderr_startup.bytes).into_owned(),
            stderr_bytes: stderr_startup.bytes,
            stdout_truncated: stdout_startup.truncated,
            stderr_truncated: stderr_startup.truncated,
            duration: start.elapsed(),
        },
        process,
    })
}

fn validate_readiness(readiness: &PersistentReadiness) -> AppResult<()> {
    if let PersistentReadiness::OutputContains(value) = readiness
        && value.is_empty()
    {
        return Err(AppError::invalid_input(
            "readiness.output",
            "output readiness marker must not be empty",
        ));
    }
    Ok(())
}

fn spawn_child(command: &Command, config: &ProcessConfig) -> AppResult<Child> {
    let mut cmd = StdCommand::new(&command.program);
    cmd.args(&command.args)
        .stdin(if command.stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    if let Some(dir) = &command.dir {
        cmd.current_dir(dir);
    }
    if command.scrub_env || !config.inherit_env {
        cmd.env_clear();
    }
    for (key, value) in &command.env {
        cmd.env(key, value);
    }
    isolate(&mut cmd);

    cmd.spawn().map_err(|error| {
        AppError::new(
            ErrorCode::Internal,
            format!("failed to spawn persistent process: {error}"),
        )
    })
}

#[allow(clippy::too_many_arguments)]
fn persistent_process(
    child: Child,
    stdin_thread: StdinThread,
    stdout_thread: ReaderThread,
    stderr_thread: ReaderThread,
    cancel_thread: Option<CancelThread>,
    cancelled: Arc<AtomicBool>,
    stdout: Capture,
    stderr: Capture,
    start: Instant,
    config: &PersistentConfig,
) -> PersistentProcess {
    PersistentProcess {
        child,
        stdin_thread,
        stdout_thread,
        stderr_thread,
        cancel_thread,
        cancelled,
        stdout,
        stderr,
        start,
        shutdown_grace_period: config.shutdown_grace_period,
        stopped: false,
    }
}

fn spawn_output_readers(
    child: &mut Child,
    stdout: &Capture,
    stderr: &Capture,
    ready_tx: &mpsc::Sender<()>,
    readiness: &PersistentReadiness,
    output: PersistentOutput,
    max_capture_bytes: Option<usize>,
) -> (ReaderThread, ReaderThread) {
    let matcher = match readiness {
        PersistentReadiness::OutputContains(value) => Some(value.clone()),
        PersistentReadiness::Started | PersistentReadiness::Command(_) => None,
    };
    let stdout_thread = child.stdout.take().map(|reader| {
        spawn_reader(
            reader,
            Arc::clone(stdout),
            matcher.clone(),
            ready_tx.clone(),
            output.stdout_stream(),
            max_capture_bytes,
        )
    });
    let stderr_thread = child.stderr.take().map(|reader| {
        spawn_reader(
            reader,
            Arc::clone(stderr),
            matcher,
            ready_tx.clone(),
            output.stderr_stream(),
            max_capture_bytes,
        )
    });
    (stdout_thread, stderr_thread)
}

fn spawn_stdin_writer(child: &mut Child, stdin: Option<Vec<u8>>) -> StdinThread {
    let bytes = stdin?;
    let stream = child.stdin.take()?;
    Some(thread::spawn(move || write_stdin(stream, bytes)))
}

fn write_stdin(mut stream: ChildStdin, bytes: Vec<u8>) -> AppResult<()> {
    match stream.write_all(&bytes) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == ErrorKind::BrokenPipe => Ok(()),
        Err(error) => Err(AppError::new(
            ErrorCode::Internal,
            format!("failed to write to persistent process stdin: {error}"),
        )),
    }
}

fn spawn_reader<R: Read + Send + 'static>(
    mut reader: R,
    capture: Capture,
    matcher: Option<String>,
    ready: mpsc::Sender<()>,
    output: Option<PersistentOutputStream>,
    max_capture_bytes: Option<usize>,
) -> thread::JoinHandle<AppResult<()>> {
    thread::spawn(move || {
        let mut buffer = [0_u8; 4096];
        let mut ready_sent = false;
        let mut match_buffer = Vec::new();
        loop {
            let read = reader.read(&mut buffer).map_err(AppError::internal)?;
            if read == 0 {
                break;
            }
            append_capture(&capture, &buffer[..read], max_capture_bytes);
            if let Some(output) = output {
                forward_output(output, &buffer[..read])?;
            }
            if !ready_sent && let Some(matcher) = &matcher {
                ready_sent = update_match_buffer(&mut match_buffer, &buffer[..read], matcher);
                if ready_sent {
                    let _ = ready.send(());
                }
            }
        }
        Ok(())
    })
}

fn forward_output(stream: PersistentOutputStream, bytes: &[u8]) -> AppResult<()> {
    match stream {
        PersistentOutputStream::Stdout => {
            let mut stdout = std::io::stdout().lock();
            stdout.write_all(bytes)?;
            stdout.flush()
        }
        PersistentOutputStream::Stderr => {
            let mut stderr = std::io::stderr().lock();
            stderr.write_all(bytes)?;
            stderr.flush()
        }
    }
    .map_err(AppError::internal)
}

fn append_capture(capture: &Capture, bytes: &[u8], max_bytes: Option<usize>) {
    let mut capture = capture.lock();
    let Some(limit) = max_bytes else {
        capture.bytes.extend_from_slice(bytes);
        return;
    };
    let remaining = limit.saturating_sub(capture.bytes.len());
    let kept = remaining.min(bytes.len());
    capture.bytes.extend_from_slice(&bytes[..kept]);
    if kept < bytes.len() {
        capture.truncated = true;
    }
}

fn take_capture(capture: &Capture) -> CapturedOutput {
    capture.lock().clone()
}

#[derive(Debug, Default, Clone)]
struct CapturedOutput {
    bytes: Vec<u8>,
    truncated: bool,
}

fn update_match_buffer(match_buffer: &mut Vec<u8>, bytes: &[u8], matcher: &str) -> bool {
    let needle = matcher.as_bytes();
    if needle.is_empty() {
        return true;
    }
    match_buffer.extend_from_slice(bytes);
    let ready_found = match_buffer
        .windows(needle.len())
        .any(|window| window == needle);
    let keep = needle.len().saturating_sub(1);
    if match_buffer.len() > keep {
        match_buffer.drain(..match_buffer.len() - keep);
    }
    ready_found
}

fn readiness_wait_error(
    child: &mut Child,
    error: mpsc::RecvTimeoutError,
    cancelled: &AtomicBool,
) -> AppResult<AppError> {
    if let Some(status) = child.try_wait().map_err(AppError::internal)? {
        if cancelled.load(Ordering::SeqCst) {
            return Ok(AppError::cancelled("persistent process startup"));
        }
        return Ok(unexpected_exit_error(status));
    }
    if cancelled.load(Ordering::SeqCst) {
        return Ok(AppError::cancelled("persistent process startup"));
    }
    match error {
        mpsc::RecvTimeoutError::Timeout => Ok(AppError::new(
            ErrorCode::Timeout,
            "persistent process did not become ready",
        )),
        mpsc::RecvTimeoutError::Disconnected => Ok(AppError::new(
            ErrorCode::Internal,
            "persistent process output ended before readiness was observed",
        )),
    }
}

fn cleanup_spawned_child(child: &mut Child, grace_period: Duration) -> AppResult<()> {
    let pid = child.id();
    if !terminate(pid) {
        let _ = child.kill();
    }
    wait_for_shutdown(child, pid, grace_period).map(|_| ())
}

fn unexpected_exit_error(status: ExitStatus) -> AppError {
    AppError::new(
        ErrorCode::Internal,
        format!("persistent process exited unexpectedly with status {status}"),
    )
}

fn run_readiness_command(
    command: &Command,
    process_config: &ProcessConfig,
    timeout: Duration,
    cancel: CancellationToken,
) -> AppResult<()> {
    let mut config = process_config.clone();
    config.timeout = Some(timeout);
    let command = command.clone();
    let result = thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|error| {
                AppError::new(
                    ErrorCode::Internal,
                    format!("failed to create readiness command runtime: {error}"),
                )
            })?;
        runtime.block_on(runner::run_with_cancel(&command, &config, cancel))
    })
    .join()
    .map_err(|_| AppError::new(ErrorCode::Internal, "readiness command runner panicked"))??;
    if result.success() {
        return Ok(());
    }
    if result.timed_out {
        return Err(AppError::new(
            ErrorCode::Timeout,
            "persistent process readiness command timed out",
        ));
    }
    Err(AppError::new(
        ErrorCode::Internal,
        "persistent process readiness command failed",
    ))
}

fn join_reader(handle: ReaderThread) -> AppResult<()> {
    if let Some(handle) = handle {
        handle
            .join()
            .map_err(|_| AppError::new(ErrorCode::Internal, "process output reader panicked"))?
    } else {
        Ok(())
    }
}

fn join_stdin(handle: StdinThread) -> AppResult<()> {
    if let Some(handle) = handle {
        handle
            .join()
            .map_err(|_| AppError::new(ErrorCode::Internal, "process stdin writer panicked"))?
    } else {
        Ok(())
    }
}

fn wait_for_shutdown(child: &mut Child, pid: u32, grace_period: Duration) -> AppResult<ExitStatus> {
    let deadline = Instant::now() + grace_period;
    loop {
        if let Some(status) = child.try_wait().map_err(AppError::internal)? {
            return Ok(status);
        }
        if Instant::now() >= deadline {
            if !kill(pid) {
                let _ = child.kill();
            }
            return child.wait().map_err(AppError::internal);
        }
        thread::sleep(Duration::from_millis(10));
    }
}

fn wait_for_exit(
    child: &mut Child,
    cancel_thread: &Option<CancelThread>,
    grace_period: Duration,
) -> AppResult<ExitStatus> {
    loop {
        if let Some(status) = child.try_wait().map_err(AppError::internal)? {
            return Ok(status);
        }
        if cancel_thread
            .as_ref()
            .is_some_and(CancelThread::is_cancel_requested)
        {
            let pid = child.id();
            if !terminate(pid) {
                let _ = child.kill();
            }
            return wait_for_shutdown(child, pid, grace_period);
        }
        thread::sleep(Duration::from_millis(10));
    }
}

#[derive(Debug)]
struct CancelThread {
    stop: CancellationToken,
    cancel: CancellationToken,
    cancelled: Arc<AtomicBool>,
    thread: thread::JoinHandle<AppResult<()>>,
}

impl CancelThread {
    fn is_cancel_requested(&self) -> bool {
        self.cancel.is_cancelled() || self.cancelled.load(Ordering::SeqCst)
    }

    fn stop(self) -> AppResult<()> {
        if self.is_cancel_requested() {
            self.cancelled.store(true, Ordering::SeqCst);
        }
        self.stop.cancel();
        self.thread
            .join()
            .map_err(|_| AppError::new(ErrorCode::Internal, "process cancel thread panicked"))?
    }
}

fn spawn_cancel_thread(
    pid: u32,
    cancel: CancellationToken,
    cancelled: Arc<AtomicBool>,
    grace_period: Duration,
) -> AppResult<CancelThread> {
    let stop = CancellationToken::new();
    let wait_stop = stop.clone();
    let wait_cancel = cancel.clone();
    let wait_cancelled = Arc::clone(&cancelled);
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| {
            AppError::new(
                ErrorCode::Internal,
                "failed to create process cancel runtime",
            )
            .with_cause(error)
        })?;
    let thread = thread::spawn(move || {
        runtime.block_on(async move {
            tokio::select! {
                biased;
                () = wait_cancel.cancelled() => {
                    wait_cancelled.store(true, Ordering::SeqCst);
                    let _ = terminate(pid);
                    tokio::select! {
                        () = wait_stop.cancelled() => Ok(()),
                        () = tokio::time::sleep(grace_period) => {
                            let _ = kill(pid);
                            Ok(())
                        }
                    }
                }
                () = wait_stop.cancelled() => Ok(()),
            }
        })
    });
    Ok(CancelThread {
        stop,
        cancel,
        cancelled,
        thread,
    })
}

#[cfg(all(test, unix))]
mod tests {
    use std::time::Duration;

    use tokio_util::sync::CancellationToken;

    use super::{
        PersistentConfig, PersistentReadiness, ShutdownOutcome, start_persistent_with_cancel,
    };
    use crate::{Command, ErrorCode, ProcessConfig};

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
        let command = Command::new("sh").arg("-c").arg("printf listening; exit 0");
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
        std::thread::sleep(Duration::from_millis(50));

        let outcome = run.process.shutdown().expect("shutdown reports outcome");
        assert!(matches!(outcome, ShutdownOutcome::AlreadyExited(_)));
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

        let run = start_persistent_with_cancel(
            &command,
            &ProcessConfig::default(),
            &config,
            cancel.clone(),
        )
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

        let run = start_persistent_with_cancel(
            &command,
            &ProcessConfig::default(),
            &config,
            cancel.clone(),
        )
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

        let error =
            start_persistent_with_cancel(&command, &ProcessConfig::default(), &config, cancel)
                .expect_err("pre-cancelled startup should fail before spawn");

        assert_eq!(error.code, ErrorCode::Cancelled);
    }

    #[test]
    fn cancellation_during_startup_returns_cancelled() {
        let command = Command::new("sh").arg("-c").arg("sleep 10");
        let config = PersistentConfig::default()
            .with_readiness(PersistentReadiness::OutputContains("ready".to_string()))
            .with_readiness_timeout(Duration::from_secs(2))
            .with_shutdown_grace_period(Duration::from_millis(50));
        let cancel = CancellationToken::new();
        let cancel_for_thread = cancel.clone();
        let cancel_thread = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(50));
            cancel_for_thread.cancel();
        });

        let error =
            start_persistent_with_cancel(&command, &ProcessConfig::default(), &config, cancel)
                .expect_err("startup cancellation should fail with cancelled semantics");
        cancel_thread.join().expect("cancel thread joins");

        assert_eq!(error.code, ErrorCode::Cancelled);
    }

    #[test]
    fn cancellation_during_command_readiness_returns_promptly() {
        let command = Command::new("sh").arg("-c").arg("sleep 10");
        let readiness = Command::new("sh").arg("-c").arg("sleep 10");
        let config = PersistentConfig::default()
            .with_readiness(PersistentReadiness::Command(readiness))
            .with_readiness_timeout(Duration::from_secs(10))
            .with_shutdown_grace_period(Duration::from_millis(50));
        let cancel = CancellationToken::new();
        let cancel_for_thread = cancel.clone();
        let cancel_thread = std::thread::spawn(move || {
            std::thread::sleep(Duration::from_millis(50));
            cancel_for_thread.cancel();
        });
        let start = std::time::Instant::now();

        let error =
            start_persistent_with_cancel(&command, &ProcessConfig::default(), &config, cancel)
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
}
