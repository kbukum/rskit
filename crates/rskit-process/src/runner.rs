//! Process execution runtime.

use crate::{AppError, AppResult, Command, ErrorCode, ProcessConfig, ProcessResult};
use std::io;
use std::time::Instant;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWriteExt};
use tokio::process::Command as TokioCommand;
use tokio::task::JoinHandle;
use tokio::time::timeout;
use tracing::{debug, warn};

/// Execute a subprocess with the given configuration.
pub async fn run(command: &Command, config: &ProcessConfig) -> AppResult<ProcessResult> {
    if command.program.is_empty() {
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

    debug!(program = %command.program, args = ?command.args, "spawning process");
    let mut child = cmd.spawn().map_err(|error| {
        AppError::new(
            ErrorCode::Internal,
            format!("failed to spawn process: {error}"),
        )
    })?;

    let stdout_task = spawn_reader(child.stdout.take(), config.max_output_bytes);
    let stderr_task = spawn_reader(child.stderr.take(), config.max_output_bytes);

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
    let (exit_code, timed_out, synthetic_stderr) = if let Some(timeout_duration) = config.timeout {
        match timeout(timeout_duration, child.wait()).await {
            Ok(Ok(status)) => (status.code(), false, None),
            Ok(Err(error)) => {
                return Err(AppError::new(
                    ErrorCode::Internal,
                    format!("process execution error: {error}"),
                ));
            }
            Err(_) => {
                debug!(program = %command.program, timeout = ?timeout_duration, "process timeout, sending SIGTERM");
                terminate_process_group(pid, libc::SIGTERM);

                match timeout(config.grace_period, child.wait()).await {
                    Ok(Ok(status)) => (status.code(), true, None),
                    Ok(Err(error)) => {
                        warn!("error waiting for process after SIGTERM: {error}");
                        terminate_process_group(pid, libc::SIGKILL);
                        (
                            None,
                            true,
                            Some(format!(
                                "process killed (error during grace period: {error})"
                            )),
                        )
                    }
                    Err(_) => {
                        debug!("grace period expired, sending SIGKILL");
                        terminate_process_group(pid, libc::SIGKILL);
                        let _ = child.wait().await;
                        (
                            None,
                            true,
                            Some("process killed by SIGKILL after timeout".to_string()),
                        )
                    }
                }
            }
        }
    } else {
        match child.wait().await {
            Ok(status) => (status.code(), false, None),
            Err(error) => {
                return Err(AppError::new(
                    ErrorCode::Internal,
                    format!("process execution error: {error}"),
                ));
            }
        }
    };

    let mut stdout = collect_reader(stdout_task).await?;
    let mut stderr = collect_reader(stderr_task).await?;
    if let Some(extra_stderr) = synthetic_stderr {
        if !stderr.is_empty() {
            stderr.push('\n');
        }
        stderr.push_str(&extra_stderr);
    }

    if config.capture_output
        && let Some(limit) = config.max_output_bytes
    {
        stdout.truncate(limit);
        stderr.truncate(limit);
    }

    let result = ProcessResult {
        exit_code,
        stdout,
        stderr,
        duration: start.elapsed(),
        timed_out,
    };

    debug!(
        exit_code = ?result.exit_code,
        duration = ?result.duration,
        timed_out = result.timed_out,
        "process completed"
    );

    Ok(result)
}

fn spawn_reader<R>(
    reader: Option<R>,
    max_output_bytes: Option<usize>,
) -> Option<JoinHandle<io::Result<String>>>
where
    R: AsyncRead + Unpin + Send + 'static,
{
    reader.map(|reader| tokio::spawn(read_output(reader, max_output_bytes)))
}

async fn collect_reader(task: Option<JoinHandle<io::Result<String>>>) -> AppResult<String> {
    match task {
        Some(task) => task
            .await
            .map_err(AppError::internal)?
            .map_err(AppError::internal),
        None => Ok(String::new()),
    }
}

async fn read_output<R>(mut reader: R, max_output_bytes: Option<usize>) -> io::Result<String>
where
    R: AsyncRead + Unpin,
{
    let mut captured = Vec::new();
    let mut buffer = [0_u8; 4096];
    let mut remaining = max_output_bytes.unwrap_or(usize::MAX);

    loop {
        let read = reader.read(&mut buffer).await?;
        if read == 0 {
            break;
        }
        if remaining > 0 {
            let to_copy = remaining.min(read);
            captured.extend_from_slice(&buffer[..to_copy]);
            remaining -= to_copy;
        }
    }

    Ok(String::from_utf8_lossy(&captured).into_owned())
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
