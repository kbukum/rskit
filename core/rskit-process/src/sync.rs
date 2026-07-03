//! Blocking subprocess execution.

use std::io::{ErrorKind, Read, Write};
use std::process::{Child, ChildStdin, Command as StdCommand, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use crate::capture::{SharedOutput, append_line_bounded, shared_output, take_shared};
use crate::process_group::kill_target;
use crate::worker::join_within;
use crate::{
    AppError, AppResult, EnvPolicy, ErrorCode, InputPolicy, OutputPolicy, ProcessConfig, ProcessIo,
    ProcessResult, ProcessSpec, SignalPolicy, terminate,
};

const POLL_INTERVAL: Duration = Duration::from_millis(10);

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

    let max_output_bytes = output.and_then(|output| output.max_output_bytes);
    let stdout_capture = shared_output();
    let stderr_capture = shared_output();
    let stdout_thread = child
        .stdout
        .take()
        .map(|stream| spawn_reader(stream, stdout_capture.clone(), max_output_bytes));
    let stderr_thread = child
        .stderr
        .take()
        .map(|stream| spawn_reader(stream, stderr_capture.clone(), max_output_bytes));
    let stdin_thread = spawn_stdin_writer(child.stdin.take(), input);

    // Own the child and its worker threads in a guard so any early return below
    // (a wait error, for example) kills the child and reaps the threads rather
    // than orphaning the child and detaching the readers, which would keep the
    // pipes open and leak the threads.
    let mut scope = BlockingChildScope::new(child, config.signal, config.signal.grace_period);
    scope.attach(stdout_thread, stderr_thread, stdin_thread);

    let pid = scope.child_mut().id();
    let (exit_code, timed_out, synthetic_stderr) =
        wait_with_timeout(scope.child_mut(), pid, config.timeout, config)?;

    // The child has exited; drain the workers within the grace period. A worker
    // still blocked because a surviving descendant holds the pipe open is
    // detached rather than joined forever.
    scope.drain()?;
    scope.disarm();

    let stdout_output = take_shared(&stdout_capture);
    let mut stderr_output = take_shared(&stderr_capture);
    if let Some(extra_stderr) = synthetic_stderr {
        stderr_output.truncated |= append_line_bounded(
            &mut stderr_output.bytes,
            extra_stderr.as_bytes(),
            max_output_bytes,
        );
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

/// Owns a spawned child and its capture/stdin worker threads so an early return
/// or panic kills the child and reaps the threads instead of leaking them.
///
/// While armed, dropping the guard best-effort kills the child (closing the
/// pipes so the readers observe EOF) and then joins each worker within the grace
/// period. [`disarm`](Self::disarm) after a normal drain hands ownership back to
/// the already-captured shared output.
struct BlockingChildScope {
    child: Child,
    stdout: Option<thread::JoinHandle<AppResult<()>>>,
    stderr: Option<thread::JoinHandle<AppResult<()>>>,
    stdin: Option<thread::JoinHandle<AppResult<()>>>,
    signal: SignalPolicy,
    grace: Duration,
    armed: bool,
}

impl BlockingChildScope {
    fn new(child: Child, signal: SignalPolicy, grace: Duration) -> Self {
        Self {
            child,
            stdout: None,
            stderr: None,
            stdin: None,
            signal,
            grace,
            armed: true,
        }
    }

    fn attach(
        &mut self,
        stdout: Option<thread::JoinHandle<AppResult<()>>>,
        stderr: Option<thread::JoinHandle<AppResult<()>>>,
        stdin: Option<thread::JoinHandle<AppResult<()>>>,
    ) {
        self.stdout = stdout;
        self.stderr = stderr;
        self.stdin = stdin;
    }

    fn child_mut(&mut self) -> &mut Child {
        &mut self.child
    }

    /// Join every worker thread within the grace period, surfacing worker
    /// errors. A worker that outlives the grace period is detached.
    fn drain(&mut self) -> AppResult<()> {
        join_within(self.stdin.take(), self.grace)?;
        join_within(self.stdout.take(), self.grace)?;
        join_within(self.stderr.take(), self.grace)
    }

    fn disarm(&mut self) {
        self.armed = false;
    }
}

impl Drop for BlockingChildScope {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        let group = terminate::targets_group(self.signal);
        if !kill_target(self.child.id(), group) {
            let _ = self.child.kill();
        }
        let _ = self.child.wait();
        let _ = join_within(self.stdout.take(), self.grace);
        let _ = join_within(self.stderr.take(), self.grace);
        let _ = join_within(self.stdin.take(), self.grace);
    }
}

fn spawn_reader<R>(
    mut reader: R,
    capture: SharedOutput,
    max_bytes: Option<usize>,
) -> thread::JoinHandle<AppResult<()>>
where
    R: Read + Send + 'static,
{
    thread::spawn(move || {
        let mut buffer = [0_u8; 4096];
        loop {
            let read = reader.read(&mut buffer).map_err(AppError::internal)?;
            if read == 0 {
                break;
            }
            capture.lock().push(&buffer[..read], max_bytes);
        }
        Ok(())
    })
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
    child: &mut Child,
    pid: u32,
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
            let (status, escalated) = terminate::terminate_and_reap(
                child,
                pid,
                config.signal,
                config.signal.grace_period,
            )?;
            let synthetic =
                escalated.then(|| "process killed by SIGKILL after timeout".to_string());
            return Ok((status.code(), true, synthetic));
        }
        thread::sleep(POLL_INTERVAL);
    }
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
    fn inherited_config_disables_descendant_signalling() {
        let config = ProcessConfig::default()
            .with_io(ProcessIo::captured(CapturedIo::new()))
            .with_timeout(None);
        let inherited = inherited_config(&config);

        assert!(!inherited.signal.create_process_group);
        assert!(!inherited.signal.terminate_descendants);
        assert_eq!(inherited.timeout, None);
    }

    #[test]
    fn join_within_reports_none_and_worker_errors() {
        join_within(None, Duration::from_millis(10)).unwrap();

        let ok = thread::spawn(|| Ok(()));
        join_within(Some(ok), Duration::from_millis(500)).unwrap();

        let failed = thread::spawn(|| Err(AppError::new(ErrorCode::Internal, "reader failed")));
        assert_eq!(
            join_within(Some(failed), Duration::from_millis(500))
                .unwrap_err()
                .code(),
            ErrorCode::Internal
        );
    }

    #[cfg(unix)]
    #[test]
    fn dropping_an_armed_scope_kills_the_child_and_reaps_workers() {
        let child = StdCommand::new("/bin/sleep")
            .arg("30")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn()
            .expect("spawn sleep");
        let pid = child.id();

        let worker = thread::spawn(|| {
            thread::sleep(Duration::from_millis(20));
            Ok(())
        });
        let mut scope = BlockingChildScope::new(
            child,
            SignalPolicy::default()
                .with_create_process_group(false)
                .with_terminate_descendants(false),
            Duration::from_millis(500),
        );
        scope.attach(Some(worker), None, None);
        drop(scope);

        // The guard killed and reaped the child, so a fresh existence probe must
        // fail with ESRCH.
        // SAFETY: signal 0 performs an existence check without delivering a
        // signal.
        let alive = unsafe { libc::kill(i32::try_from(pid).unwrap(), 0) };
        assert_eq!(alive, -1, "guard drop must kill the child");
    }
}
