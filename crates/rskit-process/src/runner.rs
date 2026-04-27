//! Process execution runtime.

#![allow(unused_imports)]

use crate::{AppError, AppResult, Command, ErrorCode, ProcessConfig, ProcessResult};
use std::io;
use std::os::unix::process::CommandExt;
use std::time::Instant;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::Command as TokioCommand;
use tokio::time::timeout;
use tracing::{debug, warn};

/// Execute a subprocess with the given configuration.
///
/// # Arguments
///
/// * `command` - The command to execute
/// * `config` - Execution configuration (timeout, grace period, etc.)
///
/// # Returns
///
/// A [`ProcessResult`] containing exit code, stdout, stderr, and duration.
/// Returns an error if the command validation fails.
///
/// # Example
///
/// ```no_run
/// use rskit_process::{Command, ProcessConfig, run};
/// use std::time::Duration;
///
/// # async fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let cmd = Command::new("echo").arg("hello");
/// let config = ProcessConfig {
///     timeout: Some(Duration::from_secs(30)),
///     ..Default::default()
/// };
/// let result = run(&cmd, &config).await?;
/// println!("Output: {}", result.stdout);
/// # Ok(())
/// # }
/// ```
pub async fn run(command: &Command, config: &ProcessConfig) -> AppResult<ProcessResult> {
    if command.program.is_empty() {
        return Err(AppError::new(
            ErrorCode::InvalidInput,
            "command program must not be empty",
        ));
    }

    let start = Instant::now();

    // Build tokio command
    let mut cmd = TokioCommand::new(&command.program);
    cmd.args(&command.args);

    // Set working directory
    if let Some(dir) = &command.dir {
        cmd.current_dir(dir);
    }

    // Set environment variables
    if config.inherit_env {
        // Merge with parent environment
        for (key, value) in &command.env {
            cmd.env(key, value);
        }
    } else {
        // Only use provided environment variables
        cmd.env_clear();
        for (key, value) in &command.env {
            cmd.env(key, value);
        }
    }

    // Capture output if requested
    if config.capture_output {
        cmd.stdout(std::process::Stdio::piped());
        cmd.stderr(std::process::Stdio::piped());
    }

    // Capture stdin if provided
    if command.stdin.is_some() {
        cmd.stdin(std::process::Stdio::piped());
    }

    // Set process group (Unix only) to enable signal propagation to child processes
    #[cfg(unix)]
    unsafe {
        cmd.pre_exec(|| {
            // Create a new process group so we can send signals to all children
            if libc::setpgid(0, 0) != 0 {
                return Err(io::Error::last_os_error());
            }
            Ok(())
        });
    }

    debug!(
        program = %command.program,
        args = ?command.args,
        "spawning process"
    );

    // Spawn the process
    let mut child = cmd.spawn().map_err(|e| {
        AppError::new(
            ErrorCode::Internal,
            format!("failed to spawn process: {}", e),
        )
    })?;

    // Write stdin if provided
    if let Some(stdin_data) = &command.stdin
        && let Some(mut stdin) = child.stdin.take()
    {
        stdin.write_all(stdin_data).await.map_err(|e| {
            AppError::new(
                ErrorCode::Internal,
                format!("failed to write to stdin: {}", e),
            )
        })?;
        drop(stdin); // Close stdin to signal EOF
    }

    // Get pid for signal handling
    let pid = child.id();

    // Wait for process with timeout handling
    let result = if let Some(timeout_duration) = config.timeout {
        match timeout(timeout_duration, child.wait()).await {
            Ok(Ok(status)) => {
                let duration = start.elapsed();
                let mut stdout = String::new();
                let mut stderr = String::new();

                if let Some(mut reader) = child.stdout.take() {
                    let _ = reader.read_to_string(&mut stdout).await;
                }
                if let Some(mut reader) = child.stderr.take() {
                    let _ = reader.read_to_string(&mut stderr).await;
                }

                ProcessResult {
                    exit_code: status.code(),
                    stdout,
                    stderr,
                    duration,
                    timed_out: false,
                }
            }
            Ok(Err(e)) => {
                return Err(AppError::new(
                    ErrorCode::Internal,
                    format!("process execution error: {}", e),
                ));
            }
            Err(_) => {
                // Timeout occurred - kill the process
                debug!(
                    program = %command.program,
                    timeout = ?timeout_duration,
                    "process timeout, sending SIGTERM"
                );

                if let Some(pid) = pid {
                    // Send SIGTERM to the process group
                    #[cfg(unix)]
                    unsafe {
                        let result = libc::kill(-(pid as i32), libc::SIGTERM);
                        if result != 0 {
                            let err = io::Error::last_os_error();
                            // ESRCH means process doesn't exist, which is fine
                            if err.raw_os_error() != Some(libc::ESRCH) {
                                warn!("failed to send SIGTERM: {}", err);
                            }
                        }
                    }
                }

                // Wait for grace period
                match timeout(config.grace_period, child.wait()).await {
                    Ok(Ok(status)) => {
                        debug!("process exited after SIGTERM");
                        let duration = start.elapsed();
                        let mut stdout = String::new();
                        let mut stderr = String::new();

                        if let Some(mut reader) = child.stdout.take() {
                            let _ = reader.read_to_string(&mut stdout).await;
                        }
                        if let Some(mut reader) = child.stderr.take() {
                            let _ = reader.read_to_string(&mut stderr).await;
                        }

                        ProcessResult {
                            exit_code: status.code(),
                            stdout,
                            stderr,
                            duration,
                            timed_out: true,
                        }
                    }
                    Ok(Err(e)) => {
                        warn!("error waiting for process after SIGTERM: {}", e);
                        // Try SIGKILL as fallback
                        if let Some(pid) = pid {
                            #[cfg(unix)]
                            unsafe {
                                let _ = libc::kill(-(pid as i32), libc::SIGKILL);
                            }
                        }
                        let duration = start.elapsed();
                        ProcessResult {
                            exit_code: None,
                            stdout: String::new(),
                            stderr: format!("process killed (error during grace period: {})", e),
                            duration,
                            timed_out: true,
                        }
                    }
                    Err(_) => {
                        // Grace period expired, send SIGKILL
                        debug!("grace period expired, sending SIGKILL");
                        if let Some(pid) = pid {
                            #[cfg(unix)]
                            unsafe {
                                let result = libc::kill(-(pid as i32), libc::SIGKILL);
                                if result != 0 {
                                    warn!("failed to send SIGKILL: {}", io::Error::last_os_error());
                                }
                            }
                        }

                        // Final wait
                        let _ = child.wait().await;
                        let duration = start.elapsed();

                        ProcessResult {
                            exit_code: None,
                            stdout: String::new(),
                            stderr: "process killed by SIGKILL after timeout".to_string(),
                            duration,
                            timed_out: true,
                        }
                    }
                }
            }
        }
    } else {
        // No timeout
        match child.wait().await {
            Ok(status) => {
                let duration = start.elapsed();
                let mut stdout = String::new();
                let mut stderr = String::new();

                if let Some(mut reader) = child.stdout.take() {
                    let _ = reader.read_to_string(&mut stdout).await;
                }
                if let Some(mut reader) = child.stderr.take() {
                    let _ = reader.read_to_string(&mut stderr).await;
                }

                ProcessResult {
                    exit_code: status.code(),
                    stdout,
                    stderr,
                    duration,
                    timed_out: false,
                }
            }
            Err(e) => {
                return Err(AppError::new(
                    ErrorCode::Internal,
                    format!("process execution error: {}", e),
                ));
            }
        }
    };

    debug!(
        exit_code = ?result.exit_code,
        duration = ?result.duration,
        timed_out = result.timed_out,
        "process completed"
    );

    Ok(result)
}
