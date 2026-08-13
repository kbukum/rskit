//! Blocking subprocess execution.

use std::io::{ErrorKind, Read, Write};
use std::process::{Child, ChildStdin, Command as StdCommand, Stdio};
use std::thread;
use std::time::{Duration, Instant};

use crate::capture::{SharedOutput, append_line_bounded, shared_output, take_shared};
use crate::process_group::kill_target;
use crate::supervisor::{OwnedChild, SyncReap, terminate_and_reap};
use crate::worker::join_within;
use crate::{
    AppError, AppResult, EnvPolicy, ErrorCode, InputPolicy, LifecyclePolicy, OutputPolicy,
    ProcessConfig, ProcessIo, ProcessResult, ProcessSpec, ProcessSupervisor, RegistrationGuard,
    command::spawn_error,
};

const POLL_INTERVAL: Duration = Duration::from_millis(10);

/// Execute a subprocess on the current thread using captured or inherited I/O mode.
///
/// Spawns through a throwaway per-call [`ProcessSupervisor`]. Callers that want
/// a shared supervisor to reap the child on process shutdown use
/// [`run_supervised`].
pub fn run(spec: &ProcessSpec, config: &ProcessConfig) -> AppResult<ProcessResult> {
    let supervisor = ProcessSupervisor::new(config.lifecycle);
    run_supervised(&supervisor, spec, config)
}

/// Execute a subprocess on the current thread, registering the spawned child
/// with `supervisor`.
///
/// Identical to [`run`] except the injected `supervisor` owns the registration,
/// so a process-level [`ProcessSupervisor::shutdown`] reaps the child even while
/// this blocking call is still waiting on it. Normal completion unregisters the
/// child through its guard.
pub fn run_supervised(
    supervisor: &ProcessSupervisor,
    spec: &ProcessSpec,
    config: &ProcessConfig,
) -> AppResult<ProcessResult> {
    if spec.program.as_os_str().is_empty() {
        return Err(AppError::invalid_input("program", "must not be empty"));
    }

    match &config.io {
        ProcessIo::Captured(io) => run_blocking(
            supervisor,
            spec,
            config,
            &io.input,
            Some(&io.output),
            pipe_stdin_stdio(&io.input)?,
        ),
        ProcessIo::Inherited(io) => run_blocking(
            supervisor,
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
    supervisor: &ProcessSupervisor,
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

    if config.lifecycle.isolate_process_group {
        crate::process_group::isolate(&mut cmd);
    }

    let mut child = cmd
        .spawn()
        .map_err(|error| spawn_error("failed to spawn process", error))?;
    let registration =
        supervisor.register_pid_with_group(child.id(), config.lifecycle.targets_group());

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

    // Own the child and its worker threads in a guard
    // so any early return below (a wait error, for example) kills the child
    // and reaps the threads rather than orphaning the child and detaching the readers,
    // which would keep the pipes open and leak the threads.
    let mut scope = BlockingChildScope::new(
        child,
        registration,
        config.lifecycle,
        config.lifecycle.grace_period,
    );
    scope.attach(stdout_thread, stderr_thread, stdin_thread);

    let (exit_code, timed_out, synthetic_stderr, survived) =
        wait_with_timeout(scope.child_mut(), config.timeout, config)?;

    // Drain the workers within the grace period. A worker still blocked because a
    // surviving descendant holds the pipe open is detached rather than joined forever.
    scope.drain()?;

    // If the child exited it is reaped, so unregister on disarm. If it deliberately
    // survived its grace period (`kill_after_grace = false`), relinquish the still-live
    // child to its owned target so a later shutdown or supervisor drop reaps it.
    if survived {
        scope.relinquish();
    } else {
        scope.disarm();
    }

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
/// While armed,
/// dropping the guard best-effort kills the child (closing the pipes so the readers observe EOF)
/// and then joins each worker within the grace period.
/// [`disarm`](Self::disarm) after a normal drain hands ownership back to the already-captured shared output.
struct BlockingChildScope {
    child: Option<Child>,
    registration: Option<RegistrationGuard>,
    stdout: Option<thread::JoinHandle<AppResult<()>>>,
    stderr: Option<thread::JoinHandle<AppResult<()>>>,
    stdin: Option<thread::JoinHandle<AppResult<()>>>,
    signal: LifecyclePolicy,
    grace: Duration,
    armed: bool,
}

impl BlockingChildScope {
    fn new(
        child: Child,
        registration: RegistrationGuard,
        signal: LifecyclePolicy,
        grace: Duration,
    ) -> Self {
        Self {
            child: Some(child),
            registration: Some(registration),
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
        match &mut self.child {
            Some(child) => child,
            // The child is removed only in `relinquish` or `Drop`, after which no
            // method is invoked on the scope, so this arm is structurally unreachable.
            None => unreachable!("BlockingChildScope::child_mut called after the child was taken"),
        }
    }

    /// Join every worker thread within the grace period, surfacing worker errors.
    /// A worker that outlives the grace period is detached.
    fn drain(&mut self) -> AppResult<()> {
        join_within(self.stdin.take(), self.grace)?;
        join_within(self.stdout.take(), self.grace)?;
        join_within(self.stderr.take(), self.grace)
    }

    fn disarm(&mut self) {
        self.armed = false;
        if let Some(registration) = self.registration.take() {
            registration.unregister();
        }
    }

    /// Relinquish a still-live child (it survived its grace period with escalation
    /// disabled) to its owned target without killing or unregistering.
    fn relinquish(&mut self) {
        self.armed = false;
        if let (Some(registration), Some(child)) = (self.registration.take(), self.child.take())
            && let Some(OwnedChild::Std(mut child)) =
                registration.relinquish_child(OwnedChild::Std(child))
        {
            let _ = child.kill();
            let _ = child.wait();
        }
    }
}

impl Drop for BlockingChildScope {
    fn drop(&mut self) {
        if !self.armed {
            return;
        }
        if let Some(mut child) = self.child.take() {
            let group = self.signal.targets_group();
            if !kill_target(child.id(), group) {
                let _ = child.kill();
            }
            let _ = child.wait();
        }
        let _ = join_within(self.stdout.take(), self.grace);
        let _ = join_within(self.stderr.take(), self.grace);
        let _ = join_within(self.stdin.take(), self.grace);
        if let Some(registration) = self.registration.take() {
            registration.unregister();
        }
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
    config.lifecycle = config
        .lifecycle
        .with_isolate_process_group(false)
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
    timeout: Option<Duration>,
    config: &ProcessConfig,
) -> AppResult<(Option<i32>, bool, Option<String>, bool)> {
    let Some(timeout) = timeout else {
        let status = child.wait().map_err(|error| {
            AppError::new(
                ErrorCode::Internal,
                format!("process execution error: {error}"),
            )
        })?;
        return Ok((status.code(), false, None, false));
    };

    let deadline = Instant::now() + timeout;
    loop {
        if let Some(status) = child.try_wait().map_err(|error| {
            AppError::new(
                ErrorCode::Internal,
                format!("process execution error: {error}"),
            )
        })? {
            return Ok((status.code(), false, None, false));
        }
        if Instant::now() >= deadline {
            return Ok(match terminate_and_reap(
                child,
                config.lifecycle,
                config.lifecycle.grace_period,
            )? {
                SyncReap::Reaped { status, escalated } => {
                    let synthetic = escalated
                        .then(|| "process killed by SIGKILL after timeout".to_string());
                    (status.code(), true, synthetic, false)
                }
                SyncReap::Survived => (
                    None,
                    true,
                    Some(
                        "grace period expired after timeout; kill escalation disabled, process left running"
                            .to_string(),
                    ),
                    true,
                ),
            });
        }
        thread::sleep(POLL_INTERVAL);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(unix)]
    use crate::pty::PtyIo;
    use crate::{CapturedIo, ObservedIo, OutputObserver, ProcessIo};

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

        assert!(!inherited.lifecycle.isolate_process_group);
        assert!(!inherited.lifecycle.terminate_descendants);
        assert_eq!(inherited.timeout, None);
    }

    #[test]
    fn blocking_run_rejects_async_only_io_modes() {
        let spec = ProcessSpec::new("true");
        let observed = ProcessConfig::default()
            .with_io(ProcessIo::observed(ObservedIo::new(OutputObserver::new())));
        assert_eq!(
            run(&spec, &observed).unwrap_err().code(),
            ErrorCode::InvalidInput
        );

        #[cfg(unix)]
        {
            let pty = ProcessConfig::default().with_io(ProcessIo::pty(PtyIo::default()));
            assert_eq!(
                run(&spec, &pty).unwrap_err().code(),
                ErrorCode::InvalidInput
            );
        }
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
        let supervisor = ProcessSupervisor::new(
            LifecyclePolicy::default()
                .with_isolate_process_group(false)
                .with_terminate_descendants(false),
        );
        let registration = supervisor.track_pid(pid);
        let mut scope = BlockingChildScope::new(
            child,
            registration,
            LifecyclePolicy::default()
                .with_isolate_process_group(false)
                .with_terminate_descendants(false),
            Duration::from_millis(500),
        );
        scope.attach(Some(worker), None, None);
        drop(scope);

        // The guard killed and reaped the child, so a fresh existence probe must fail with ESRCH.
        // SAFETY: signal 0 performs an existence check without delivering a signal.
        let alive = unsafe { libc::kill(i32::try_from(pid).unwrap(), 0) };
        assert_eq!(alive, -1, "guard drop must kill the child");
    }
}
