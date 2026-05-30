//! Blocking subprocess execution.

use std::io::{Read, Write};
use std::process::{Command as StdCommand, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use crate::{AppError, AppResult, Command, ErrorCode, ProcessConfig, ProcessResult};

/// Execute a subprocess on the current thread using the shared rskit process policy.
pub fn run(command: &Command, config: &ProcessConfig) -> AppResult<ProcessResult> {
    if command.program.as_os_str().is_empty() {
        return Err(AppError::invalid_input("program", "must not be empty"));
    }

    let start = Instant::now();
    let mut cmd = StdCommand::new(&command.program);
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
        cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
    }
    if command.stdin.is_some() {
        cmd.stdin(Stdio::piped());
    } else {
        cmd.stdin(Stdio::null());
    }

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;

        // SAFETY: `pre_exec` runs in the child process after fork and before exec.
        // The closure only calls async-signal-safe `setpgid` and returns an
        // `io::Error` on failure, matching the async runner's process-group policy.
        unsafe {
            cmd.pre_exec(|| {
                if libc::setpgid(0, 0) != 0 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
    }

    let mut child = cmd.spawn().map_err(|error| {
        AppError::new(
            ErrorCode::Internal,
            format!("failed to spawn process: {error}"),
        )
    })?;
    let pid = Some(child.id());

    if let Some(stdin_data) = &command.stdin
        && let Some(mut stdin) = child.stdin.take()
    {
        stdin.write_all(stdin_data).map_err(|error| {
            AppError::new(
                ErrorCode::Internal,
                format!("failed to write to stdin: {error}"),
            )
        })?;
    }

    let stdout = child
        .stdout
        .take()
        .map(|stream| spawn_reader(stream, config.max_output_bytes));
    let stderr = child
        .stderr
        .take()
        .map(|stream| spawn_reader(stream, config.max_output_bytes));

    let (exit_code, timed_out, synthetic_stderr) =
        wait_with_timeout(&mut child, pid, config.timeout, config.grace_period)?;
    let stdout_output = join_reader(stdout)?;
    let mut stderr_output = join_reader(stderr)?;
    if let Some(extra_stderr) = synthetic_stderr {
        stderr_output.truncated |= append_bounded(
            &mut stderr_output.bytes,
            extra_stderr.as_bytes(),
            config.max_output_bytes,
        );
    }

    Ok(ProcessResult {
        exit_code,
        stdout: String::from_utf8_lossy(&stdout_output.bytes).into_owned(),
        stdout_bytes: stdout_output.bytes,
        stderr: String::from_utf8_lossy(&stderr_output.bytes).into_owned(),
        stderr_bytes: stderr_output.bytes,
        stdout_truncated: stdout_output.truncated,
        stderr_truncated: stderr_output.truncated,
        duration: start.elapsed(),
        timed_out,
        cancelled: false,
    })
}

fn wait_with_timeout(
    child: &mut std::process::Child,
    pid: Option<u32>,
    timeout: Option<Duration>,
    grace_period: Duration,
) -> AppResult<(Option<i32>, bool, Option<String>)> {
    let Some(timeout) = timeout else {
        let status = child.wait().map_err(|error| {
            AppError::new(
                ErrorCode::Internal,
                format!("process execution error: {error}"),
            )
        })?;
        return Ok((status.code(), false, None));
    };

    let deadline = Instant::now() + timeout;
    loop {
        if let Some(status) = child.try_wait().map_err(|error| {
            AppError::new(
                ErrorCode::Internal,
                format!("process execution error: {error}"),
            )
        })? {
            return Ok((status.code(), false, None));
        }
        if Instant::now() >= deadline {
            return terminate_and_wait(child, pid, grace_period);
        }
        thread::sleep(Duration::from_millis(10));
    }
}

fn terminate_and_wait(
    child: &mut std::process::Child,
    pid: Option<u32>,
    grace_period: Duration,
) -> AppResult<(Option<i32>, bool, Option<String>)> {
    if !terminate_process_group(pid, libc::SIGTERM) && !terminate_process(pid, libc::SIGTERM) {
        child.kill().map_err(|error| {
            AppError::new(
                ErrorCode::Internal,
                format!("failed to terminate process after timeout: {error}"),
            )
        })?;
    }

    let deadline = Instant::now() + grace_period;
    loop {
        if let Some(status) = child.try_wait().map_err(|error| {
            AppError::new(
                ErrorCode::Internal,
                format!("process execution error after timeout: {error}"),
            )
        })? {
            return Ok((status.code(), true, None));
        }
        if Instant::now() >= deadline {
            if !terminate_process_group(pid, libc::SIGKILL)
                && !terminate_process(pid, libc::SIGKILL)
            {
                child.kill().map_err(|error| {
                    AppError::new(
                        ErrorCode::Internal,
                        format!("failed to kill process after timeout: {error}"),
                    )
                })?;
            }
            let status = child.wait().map_err(|error| {
                AppError::new(
                    ErrorCode::Internal,
                    format!("process execution error after kill: {error}"),
                )
            })?;
            return Ok((
                status.code(),
                true,
                Some("process killed by SIGKILL after timeout".to_string()),
            ));
        }
        thread::sleep(Duration::from_millis(10));
    }
}

struct ReaderOutput {
    bytes: Vec<u8>,
    truncated: bool,
}

fn spawn_reader<R>(
    mut reader: R,
    max_bytes: Option<usize>,
) -> thread::JoinHandle<std::io::Result<ReaderOutput>>
where
    R: Read + Send + 'static,
{
    thread::spawn(move || {
        let mut bytes = Vec::new();
        let mut buffer = [0_u8; 4096];
        let mut remaining = max_bytes.unwrap_or(usize::MAX);
        let mut truncated = false;
        loop {
            let read = reader.read(&mut buffer)?;
            if read == 0 {
                break;
            }
            if remaining > 0 {
                let to_copy = remaining.min(read);
                bytes.extend_from_slice(&buffer[..to_copy]);
                remaining -= to_copy;
                if to_copy < read {
                    truncated = true;
                }
            } else {
                truncated = true;
            }
        }
        Ok(ReaderOutput { bytes, truncated })
    })
}

fn join_reader(
    handle: Option<thread::JoinHandle<std::io::Result<ReaderOutput>>>,
) -> AppResult<ReaderOutput> {
    match handle {
        Some(handle) => handle
            .join()
            .map_err(|_| {
                AppError::new(ErrorCode::Internal, "process output reader thread panicked")
            })?
            .map_err(AppError::internal),
        None => Ok(ReaderOutput {
            bytes: Vec::new(),
            truncated: false,
        }),
    }
}

fn append_bounded(target: &mut Vec<u8>, extra: &[u8], max_bytes: Option<usize>) -> bool {
    let Some(limit) = max_bytes else {
        if !target.is_empty() {
            target.push(b'\n');
        }
        target.extend_from_slice(extra);
        return false;
    };
    if target.len() >= limit {
        return true;
    }
    if !target.is_empty() && target.len() + 1 < limit {
        target.push(b'\n');
    }
    let remaining = limit.saturating_sub(target.len());
    if extra.len() > remaining {
        target.extend_from_slice(&extra[..remaining]);
        true
    } else {
        target.extend_from_slice(extra);
        false
    }
}

fn terminate_process_group(pid: Option<u32>, signal: i32) -> bool {
    if let Some(pid) = pid {
        #[cfg(unix)]
        // SAFETY: `kill` targets the negated process-group id created by the
        // `pre_exec` hook above. ESRCH means the group already exited.
        unsafe {
            let result = libc::kill(-(pid as i32), signal);
            if result != 0 {
                let error = std::io::Error::last_os_error();
                if error.raw_os_error() != Some(libc::ESRCH) {
                    return false;
                }
            }
            return true;
        }
    }
    false
}

fn terminate_process(pid: Option<u32>, signal: i32) -> bool {
    if let Some(pid) = pid {
        #[cfg(unix)]
        // SAFETY: `kill` targets a single child process id as a fallback when
        // process-group signalling is unavailable. ESRCH means it already exited.
        unsafe {
            let result = libc::kill(pid as i32, signal);
            if result != 0 {
                let error = std::io::Error::last_os_error();
                if error.raw_os_error() != Some(libc::ESRCH) {
                    return false;
                }
            }
            return true;
        }
    }
    false
}
