//! Process execution runtime.

use crate::{
    AppError, AppResult, Command, ErrorCode, ProcessConfig, ProcessResult,
    command::DEFAULT_MAX_OUTPUT_BYTES,
};
use std::io;
use std::sync::Arc;
use std::time::Instant;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::process::Child;
use tokio::process::Command as TokioCommand;
use tokio::task::JoinHandle;
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;
use tracing::{debug, warn};

/// Callback invoked for line-oriented process output.
pub type OutputLineCallback = Arc<dyn Fn(&str) + Send + Sync + 'static>;

/// Optional callbacks for line-oriented process output.
#[derive(Clone, Default)]
pub struct OutputObserver {
    stdout_line: Option<OutputLineCallback>,
    stderr_line: Option<OutputLineCallback>,
}

impl OutputObserver {
    /// Create an observer without callbacks.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Observe each stdout line.
    #[must_use]
    pub fn with_stdout_line(mut self, callback: impl Fn(&str) + Send + Sync + 'static) -> Self {
        self.stdout_line = Some(Arc::new(callback));
        self
    }

    /// Observe each stderr line.
    #[must_use]
    pub fn with_stderr_line(mut self, callback: impl Fn(&str) + Send + Sync + 'static) -> Self {
        self.stderr_line = Some(Arc::new(callback));
        self
    }
}

/// Execute a subprocess with the given configuration and cancellation token.
pub async fn run_with_cancel(
    command: &Command,
    config: &ProcessConfig,
    cancel: CancellationToken,
) -> AppResult<ProcessResult> {
    run_process(command, config, cancel, None).await
}

/// Execute a subprocess and observe stdout/stderr lines as they are emitted.
pub async fn run_with_observer(
    command: &Command,
    config: &ProcessConfig,
    cancel: CancellationToken,
    observer: OutputObserver,
) -> AppResult<ProcessResult> {
    run_process(command, config, cancel, Some(observer)).await
}

async fn run_process(
    command: &Command,
    config: &ProcessConfig,
    cancel: CancellationToken,
    observer: Option<OutputObserver>,
) -> AppResult<ProcessResult> {
    if command.program.as_os_str().is_empty() {
        return Err(AppError::invalid_input("program", "must not be empty"));
    }

    let start = Instant::now();
    let mut cmd = TokioCommand::new(&command.program);
    cmd.args(&command.args);

    if let Some(dir) = &command.dir {
        cmd.current_dir(dir);
    }

    if command.scrub_env || !config.inherit_env {
        cmd.env_clear();
    }
    for (key, value) in &command.env {
        cmd.env(key, value);
    }

    if config.capture_output {
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());
    }
    if command.stdin.is_some() {
        cmd.stdin(std::process::Stdio::piped());
    } else {
        cmd.stdin(std::process::Stdio::null());
    }

    #[cfg(unix)]
    // SAFETY: `pre_exec` runs in the child process after fork and before exec.
    // The closure only calls the async-signal-safe `setpgid` libc function and
    // returns an `io::Error` on failure, which is the supported usage pattern.
    unsafe {
        cmd.pre_exec(|| {
            if libc::setpgid(0, 0) != 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }

    debug!(program = %command.program.display(), args = ?command.args, "spawning process");
    let mut child = cmd.spawn().map_err(|error| {
        AppError::new(
            ErrorCode::Internal,
            format!("failed to spawn process: {error}"),
        )
    })?;

    let stdout_task = spawn_reader(
        child.stdout.take(),
        config.max_output_bytes,
        observer
            .as_ref()
            .and_then(|observer| observer.stdout_line.clone()),
    );
    let stderr_task = spawn_reader(
        child.stderr.take(),
        config.max_output_bytes,
        observer
            .as_ref()
            .and_then(|observer| observer.stderr_line.clone()),
    );

    if let Some(stdin_data) = &command.stdin
        && let Some(mut stdin) = child.stdin.take()
    {
        stdin.write_all(stdin_data).await.map_err(|error| {
            AppError::new(
                ErrorCode::Internal,
                format!("failed to write to stdin: {error}"),
            )
        })?;
    }

    let pid = child.id();
    let (exit_code, timed_out, cancelled, synthetic_stderr) = if let Some(timeout_duration) =
        config.timeout
    {
        tokio::select! {
            _ = cancel.cancelled() => {
                debug!(program = %command.program.display(), "process cancelled, sending SIGTERM");
                let (exit_code, stderr) = terminate_and_wait(&mut child, pid, config.grace_period, "cancellation").await;
                (exit_code, false, true, stderr)
            }
            wait_result = timeout(timeout_duration, child.wait()) => {
                match wait_result {
                    Ok(Ok(status)) => (status.code(), false, false, None),
                    Ok(Err(error)) => {
                        return Err(AppError::new(
                            ErrorCode::Internal,
                            format!("process execution error: {error}"),
                        ));
                    }
                    Err(_) => {
                        debug!(program = %command.program.display(), timeout = ?timeout_duration, "process timeout, sending SIGTERM");
                        let (exit_code, stderr) = terminate_and_wait(&mut child, pid, config.grace_period, "timeout").await;
                        (exit_code, true, false, stderr)
                    }
                }
            }
        }
    } else {
        tokio::select! {
            _ = cancel.cancelled() => {
                debug!(program = %command.program.display(), "process cancelled, sending SIGTERM");
                let (exit_code, stderr) = terminate_and_wait(&mut child, pid, config.grace_period, "cancellation").await;
                (exit_code, false, true, stderr)
            }
            wait_result = child.wait() => {
                match wait_result {
                    Ok(status) => (status.code(), false, false, None),
                    Err(error) => {
                        return Err(AppError::new(
                            ErrorCode::Internal,
                            format!("process execution error: {error}"),
                        ));
                    }
                }
            }
        }
    };

    let stdout_output = collect_reader(stdout_task).await?;
    let stdout_bytes = stdout_output.bytes;
    let stdout = String::from_utf8_lossy(&stdout_bytes).into_owned();
    let stdout_truncated = stdout_output.truncated;
    let stderr_output = collect_reader(stderr_task).await?;
    let mut stderr_bytes = stderr_output.bytes;
    let stderr_truncated = stderr_output.truncated;
    if let Some(extra_stderr) = synthetic_stderr {
        if !stderr_bytes.is_empty() {
            stderr_bytes.push(b'\n');
        }
        stderr_bytes.extend_from_slice(extra_stderr.as_bytes());
    }
    let stderr = String::from_utf8_lossy(&stderr_bytes).into_owned();

    let result = ProcessResult {
        exit_code,
        stdout,
        stdout_bytes,
        stderr,
        stderr_bytes,
        stdout_truncated,
        stderr_truncated,
        duration: start.elapsed(),
        timed_out,
    };

    debug!(
        exit_code = ?result.exit_code,
        duration = ?result.duration,
        timed_out = result.timed_out,
        "process completed"
    );

    if cancelled {
        let mut error = AppError::new(ErrorCode::Cancelled, "process cancelled")
            .with_detail("timed_out", result.timed_out)
            .with_detail("duration_ms", result.duration.as_millis() as u64)
            .with_detail("stdout", result.stdout.clone())
            .with_detail("stderr", result.stderr.clone());
        if let Some(exit_code) = result.exit_code {
            error = error.with_detail("exit_code", exit_code);
        }
        return Err(error);
    }

    Ok(result)
}

async fn terminate_and_wait(
    child: &mut Child,
    pid: Option<u32>,
    grace_period: std::time::Duration,
    reason: &str,
) -> (Option<i32>, Option<String>) {
    terminate_process_group(pid, libc::SIGTERM);
    match timeout(grace_period, child.wait()).await {
        Ok(Ok(status)) => (status.code(), None),
        Ok(Err(error)) => {
            warn!("error waiting for process after SIGTERM: {error}");
            terminate_process_group(pid, libc::SIGKILL);
            (
                None,
                Some(format!(
                    "process killed (error during grace period after {reason}: {error})"
                )),
            )
        }
        Err(_) => {
            debug!("grace period expired, sending SIGKILL");
            terminate_process_group(pid, libc::SIGKILL);
            let _ = child.wait().await;
            (
                None,
                Some(format!("process killed by SIGKILL after {reason}")),
            )
        }
    }
}

fn spawn_reader<R>(
    reader: Option<R>,
    max_output_bytes: Option<usize>,
    line_callback: Option<OutputLineCallback>,
) -> Option<JoinHandle<io::Result<CapturedOutput>>>
where
    R: AsyncRead + Unpin + Send + 'static,
{
    reader.map(|reader| match line_callback {
        Some(callback) => tokio::spawn(read_observed_lines(reader, max_output_bytes, callback)),
        None => tokio::spawn(read_output(reader, max_output_bytes)),
    })
}

#[derive(Debug)]
struct CapturedOutput {
    bytes: Vec<u8>,
    truncated: bool,
}

async fn collect_reader(
    task: Option<JoinHandle<io::Result<CapturedOutput>>>,
) -> AppResult<CapturedOutput> {
    match task {
        Some(task) => task
            .await
            .map_err(AppError::internal)?
            .map_err(AppError::internal),
        None => Ok(CapturedOutput {
            bytes: Vec::new(),
            truncated: false,
        }),
    }
}

async fn read_output<R>(
    mut reader: R,
    max_output_bytes: Option<usize>,
) -> io::Result<CapturedOutput>
where
    R: AsyncRead + Unpin,
{
    let mut captured = Vec::new();
    let mut buffer = [0_u8; 4096];
    let mut remaining = max_output_bytes.unwrap_or(usize::MAX);
    let mut truncated = false;

    loop {
        let read = reader.read(&mut buffer).await?;
        if read == 0 {
            break;
        }
        if remaining > 0 {
            let to_copy = remaining.min(read);
            captured.extend_from_slice(&buffer[..to_copy]);
            remaining -= to_copy;
            if to_copy < read {
                truncated = true;
            }
        } else {
            truncated = true;
        }
    }

    Ok(CapturedOutput {
        bytes: captured,
        truncated,
    })
}

async fn read_observed_lines<R>(
    reader: R,
    max_output_bytes: Option<usize>,
    line_callback: OutputLineCallback,
) -> io::Result<CapturedOutput>
where
    R: AsyncRead + Unpin,
{
    let mut reader = tokio::io::BufReader::new(reader);
    let mut captured = Vec::new();
    let mut remaining = max_output_bytes.unwrap_or(usize::MAX);
    let mut line = Vec::new();
    let max_line_bytes = max_output_bytes.unwrap_or(DEFAULT_MAX_OUTPUT_BYTES);
    let mut line_truncated = false;
    let mut buffer = [0_u8; 4096];
    let mut capture_truncated = false;

    loop {
        let read = reader.read(&mut buffer).await?;
        if read == 0 {
            if !line.is_empty() && !line_truncated {
                emit_observed_line(&line, &line_callback);
            }
            break;
        }

        if remaining > 0 {
            let to_copy = remaining.min(read);
            captured.extend_from_slice(&buffer[..to_copy]);
            remaining -= to_copy;
            if to_copy < read {
                capture_truncated = true;
            }
        } else {
            capture_truncated = true;
        }

        for byte in &buffer[..read] {
            if *byte == b'\n' {
                if !line_truncated {
                    line.push(*byte);
                    emit_observed_line(&line, &line_callback);
                }
                line.clear();
                line_truncated = false;
                continue;
            }

            if line_truncated {
                continue;
            }

            if line.len() < max_line_bytes {
                line.push(*byte);
            } else {
                emit_observed_line(&line, &line_callback);
                line.clear();
                line_truncated = true;
            }
        }
    }

    Ok(CapturedOutput {
        bytes: captured,
        truncated: capture_truncated,
    })
}

fn emit_observed_line(line: &[u8], line_callback: &OutputLineCallback) {
    let observed = String::from_utf8_lossy(line);
    let observed = observed.trim_end_matches(['\r', '\n']);
    line_callback(observed);
}

fn terminate_process_group(pid: Option<u32>, signal: i32) {
    if let Some(pid) = pid {
        #[cfg(unix)]
        // SAFETY: `kill` is invoked with the negated process-group id created by
        // the `pre_exec` hook above so signals fan out to the subprocess tree.
        // Errors are handled explicitly and ignored only for `ESRCH`.
        unsafe {
            let result = libc::kill(-(pid as i32), signal);
            if result != 0 {
                let error = io::Error::last_os_error();
                if error.raw_os_error() != Some(libc::ESRCH) {
                    warn!(signal, "failed to send signal: {error}");
                }
            }
        }
    }
}
