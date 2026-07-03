//! Blocking subprocess execution.

use std::io::{ErrorKind, Read, Write};
use std::process::{ChildStdin, Command as StdCommand, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use crate::{
    AppError, AppResult, EnvPolicy, ErrorCode, InputPolicy, OutputPolicy, ProcessConfig, ProcessIo,
    ProcessResult, ProcessSpec,
};

/// Execute a subprocess on the current thread using captured or inherited I/O mode.
pub fn run(spec: &ProcessSpec, config: &ProcessConfig) -> AppResult<ProcessResult> {
    if spec.program.as_os_str().is_empty() {
        return Err(AppError::invalid_input("program", "must not be empty"));
    }

    match &config.io {
        ProcessIo::Captured(io) => run_blocking(
            spec,
            config,
            &io.input,
            Some(&io.output),
            pipe_stdin_stdio(&io.input)?,
        ),
        ProcessIo::Inherited(io) => run_blocking(
            spec,
            &inherited_config(config),
            &io.input,
            None,
            stdin_stdio(&io.input),
        ),
        ProcessIo::Observed(_) => Err(AppError::invalid_input(
            "process.io",
            "observed mode requires async run_with_cancel",
        )),
        #[cfg(unix)]
        ProcessIo::Pty(_) => Err(AppError::invalid_input(
            "process.io",
            "pty mode requires async run_with_cancel",
        )),
    }
}

fn run_blocking(
    spec: &ProcessSpec,
    config: &ProcessConfig,
    input: &InputPolicy,
    output: Option<&OutputPolicy>,
    stdin: Stdio,
) -> AppResult<ProcessResult> {
    let start = Instant::now();
    let mut cmd = StdCommand::new(&spec.program);
    cmd.args(&spec.args)
        .stdin(stdin)
        .stdout(stdout_stdio(output))
        .stderr(stderr_stdio(output));

    if let Some(dir) = &spec.dir {
        cmd.current_dir(dir);
    }

    if matches!(spec.env_policy, EnvPolicy::Empty) {
        cmd.env_clear();
    }
    for (key, value) in &spec.env {
        cmd.env(key, value);
    }

    if config.signal.create_process_group {
        crate::process_group::isolate(&mut cmd);
    }

    let mut child = cmd.spawn().map_err(|error| {
        AppError::new(
            ErrorCode::Internal,
            format!("failed to spawn process: {error}"),
        )
    })?;
    let pid = Some(child.id());

    let max_output_bytes = output.and_then(|output| output.max_output_bytes);
    let stdout = child
        .stdout
        .take()
        .map(|stream| spawn_reader(stream, max_output_bytes));
    let stderr = child
        .stderr
        .take()
        .map(|stream| spawn_reader(stream, max_output_bytes));
    let stdin = spawn_stdin_writer(child.stdin.take(), input);

    let (exit_code, timed_out, synthetic_stderr) =
        wait_with_timeout(&mut child, pid, config.timeout, config)?;
    join_stdin(stdin)?;
    let stdout_output = join_reader(stdout)?;
    let mut stderr_output = join_reader(stderr)?;
    if let Some(extra_stderr) = synthetic_stderr {
        stderr_output.truncated |= append_bounded(
            &mut stderr_output.bytes,
            extra_stderr.as_bytes(),
            max_output_bytes,
        );
    }

    fn spawn_stdin_writer(
        stdin: Option<ChildStdin>,
        input: &InputPolicy,
    ) -> Option<thread::JoinHandle<AppResult<()>>> {
        let InputPolicy::Bytes(bytes) = input else {
            return None;
        };
        let mut stdin = stdin?;
        let bytes = bytes.clone();
        Some(thread::spawn(move || match stdin.write_all(&bytes) {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == ErrorKind::BrokenPipe => Ok(()),
            Err(error) => Err(AppError::new(
                ErrorCode::Internal,
                format!("failed to write to stdin: {error}"),
            )),
        }))
    }

    Ok(ProcessResult::completed(
        exit_code,
        stdout_output.bytes,
        stderr_output.bytes,
        stdout_output.truncated,
        stderr_output.truncated,
        start.elapsed(),
        timed_out,
        false,
    ))
}

fn stdin_stdio(input: &InputPolicy) -> Stdio {
    match input {
        InputPolicy::Closed => Stdio::null(),
        InputPolicy::Bytes(_) => Stdio::piped(),
        InputPolicy::Inherit => Stdio::inherit(),
    }
}

fn pipe_stdin_stdio(input: &InputPolicy) -> AppResult<Stdio> {
    match input {
        InputPolicy::Closed => Ok(Stdio::null()),
        InputPolicy::Bytes(_) => Ok(Stdio::piped()),
        InputPolicy::Inherit => Err(AppError::invalid_input(
            "process.io.input",
            "inherited stdin requires inherited I/O mode; pipe-backed interactive stdin is not supported",
        )),
    }
}

fn inherited_config(config: &ProcessConfig) -> ProcessConfig {
    let mut config = config.clone();
    config.signal = config
        .signal
        .with_create_process_group(false)
        .with_terminate_descendants(false);
    config
}

fn stdout_stdio(output: Option<&OutputPolicy>) -> Stdio {
    match output {
        Some(output) if output.capture_stdout => Stdio::piped(),
        Some(_) => Stdio::null(),
        None => Stdio::inherit(),
    }
}

fn stderr_stdio(output: Option<&OutputPolicy>) -> Stdio {
    match output {
        Some(output) if output.capture_stderr => Stdio::piped(),
        Some(_) => Stdio::null(),
        None => Stdio::inherit(),
    }
}

fn wait_with_timeout(
    child: &mut std::process::Child,
    pid: Option<u32>,
    timeout: Option<Duration>,
    config: &ProcessConfig,
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
            return terminate_and_wait(child, pid, config);
        }
        thread::sleep(Duration::from_millis(10));
    }
}

fn terminate_and_wait(
    child: &mut std::process::Child,
    pid: Option<u32>,
    config: &ProcessConfig,
) -> AppResult<(Option<i32>, bool, Option<String>)> {
    if !terminate_process(pid, config, libc::SIGTERM) {
        child.kill().map_err(|error| {
            AppError::new(
                ErrorCode::Internal,
                format!("failed to terminate process after timeout: {error}"),
            )
        })?;
    }

    let deadline = Instant::now() + config.signal.grace_period;
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
            if !terminate_process(pid, config, libc::SIGKILL) {
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

fn join_stdin(handle: Option<thread::JoinHandle<AppResult<()>>>) -> AppResult<()> {
    match handle {
        Some(handle) => handle
            .join()
            .map_err(|_| AppError::new(ErrorCode::Internal, "process stdin writer panicked"))?,
        None => Ok(()),
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

fn terminate_process(pid: Option<u32>, config: &ProcessConfig, signal: i32) -> bool {
    if let Some(pid) = pid {
        #[cfg(unix)]
        // SAFETY: `kill` targets either the child pid or the negated
        // process-group id created by the `pre_exec` hook. ESRCH means it
        // already exited.
        unsafe {
            let target =
                if config.signal.create_process_group && config.signal.terminate_descendants {
                    -(pid as i32)
                } else {
                    pid as i32
                };
            let result = libc::kill(target, signal);
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{CapturedIo, ProcessIo};

    #[test]
    fn stdio_helpers_map_input_and_output_policies() {
        assert!(pipe_stdin_stdio(&InputPolicy::Closed).is_ok());
        assert!(pipe_stdin_stdio(&InputPolicy::Bytes(b"x".to_vec())).is_ok());
        assert_eq!(
            pipe_stdin_stdio(&InputPolicy::Inherit).unwrap_err().code(),
            ErrorCode::InvalidInput
        );

        let captured = OutputPolicy::captured();
        let _ = stdout_stdio(Some(&captured));
        let _ = stderr_stdio(Some(&captured));
        let discarded = OutputPolicy::observe_only();
        let _ = stdout_stdio(Some(&discarded));
        let _ = stderr_stdio(Some(&discarded));
        let _ = stdout_stdio(None);
        let _ = stderr_stdio(None);
        let _ = stdin_stdio(&InputPolicy::Closed);
        let _ = stdin_stdio(&InputPolicy::Bytes(Vec::new()));
        let _ = stdin_stdio(&InputPolicy::Inherit);
    }

    #[test]
    fn inherited_config_disables_descendant_signalling_and_join_none_is_ok() {
        let config = ProcessConfig::default()
            .with_io(ProcessIo::captured(CapturedIo::new()))
            .with_timeout(None);
        let inherited = inherited_config(&config);

        assert!(!inherited.signal.create_process_group);
        assert!(!inherited.signal.terminate_descendants);
        assert_eq!(inherited.timeout, None);
        join_stdin(None).unwrap();
        let output = join_reader(None).unwrap();
        assert!(output.bytes.is_empty());
        assert!(!output.truncated);
        assert!(!terminate_process(None, &config, libc::SIGTERM));
    }

    #[test]
    fn append_bounded_preserves_separator_and_reports_truncation() {
        let mut bytes = b"abc".to_vec();
        assert!(!append_bounded(&mut bytes, b"def", None));
        assert_eq!(bytes, b"abc\ndef");

        let mut limited = b"abc".to_vec();
        assert!(!append_bounded(&mut limited, b"de", Some(6)));
        assert_eq!(limited, b"abc\nde");
        assert!(append_bounded(&mut limited, b"f", Some(6)));

        let mut already_full = b"abcdef".to_vec();
        assert!(append_bounded(&mut already_full, b"g", Some(6)));
        assert_eq!(already_full, b"abcdef");
    }

    #[test]
    fn join_helpers_report_panicked_threads() {
        let reader = std::thread::spawn(|| -> std::io::Result<ReaderOutput> {
            panic!("reader panic");
        });
        let stdin = std::thread::spawn(|| -> AppResult<()> {
            panic!("stdin panic");
        });

        let reader_error = match join_reader(Some(reader)) {
            Ok(_) => panic!("reader panic should map to error"),
            Err(error) => error,
        };
        let stdin_error = match join_stdin(Some(stdin)) {
            Ok(_) => panic!("stdin panic should map to error"),
            Err(error) => error,
        };

        assert_eq!(reader_error.code(), ErrorCode::Internal);
        assert_eq!(stdin_error.code(), ErrorCode::Internal);
    }

    #[cfg(unix)]
    #[test]
    fn terminate_process_treats_esrch_as_already_exited() {
        // Spawn and reap a short-lived child so the PID is guaranteed dead,
        // then probe with signal 0 (existence check) instead of SIGTERM so
        // PID-reuse cannot cause this test to deliver a real signal to an
        // unrelated process.
        let mut child = std::process::Command::new("/usr/bin/true")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("spawning /usr/bin/true succeeds");
        let pid = child.id();
        child.wait().expect("child exits and is reaped");

        let config = ProcessConfig::default();
        assert!(terminate_process(Some(pid), &config, 0));
    }
}
