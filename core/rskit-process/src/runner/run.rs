use std::process::Stdio;
use std::time::Instant;

use tokio::process::Command as TokioCommand;
use tokio_util::sync::CancellationToken;
use tracing::debug;

use crate::{
    AppError, AppResult, InheritedIo, InputPolicy, OutputPolicy, ProcessConfig, ProcessIo,
    ProcessResult, ProcessSpec,
    capture::{append_line_bounded, shared_output},
    command::spawn_error,
};

use super::lifecycle::wait_for_completion;
use super::observer::OutputObserver;
use super::output::{captured, join_within, spawn_reader};
use super::pipe_io::{PipeIo, inherited_config, pipe_stdin_stdio, spawn_stdin_writer, stdin_stdio};
#[cfg(unix)]
use super::pty::run_pty_mode;
use super::redaction::RedactedArgs;
use super::scope::ChildScope;
use super::spawn::{PipeStdio, configure_command};

/// Execute a subprocess with the given configuration and cancellation token.
pub async fn run_with_cancel(
    spec: &ProcessSpec,
    config: &ProcessConfig,
    cancel: CancellationToken,
) -> AppResult<ProcessResult> {
    match &config.io {
        ProcessIo::Captured(io) => run_pipe_mode(spec, config, cancel, io, None).await,
        ProcessIo::Observed(io) => {
            run_pipe_mode(spec, config, cancel, io, Some(io.observer.clone())).await
        }
        ProcessIo::Inherited(io) => run_inherited_mode(spec, config, cancel, io).await,
        #[cfg(unix)]
        ProcessIo::Pty(io) => run_pty_mode(spec, config, cancel, io).await,
    }
}

async fn run_pipe_mode(
    spec: &ProcessSpec,
    config: &ProcessConfig,
    cancel: CancellationToken,
    io: &impl PipeIo,
    observer: Option<OutputObserver>,
) -> AppResult<ProcessResult> {
    let stdout_observer = observer
        .as_ref()
        .and_then(|observer| observer.stdout_line.clone());
    let stderr_observer = observer
        .as_ref()
        .and_then(|observer| observer.stderr_line.clone());
    let stdout_bytes_observer = observer
        .as_ref()
        .and_then(|observer| observer.stdout_bytes.clone());
    let stderr_bytes_observer = observer
        .as_ref()
        .and_then(|observer| observer.stderr_bytes.clone());
    let output = io.output();
    let pipe_stdout =
        output.capture_stdout || stdout_observer.is_some() || stdout_bytes_observer.is_some();
    let pipe_stderr =
        output.capture_stderr || stderr_observer.is_some() || stderr_bytes_observer.is_some();

    let stdio = PipeStdio {
        stdin: pipe_stdin_stdio(io.input())?,
        stdout: if pipe_stdout {
            Stdio::piped()
        } else {
            Stdio::null()
        },
        stderr: if pipe_stderr {
            Stdio::piped()
        } else {
            Stdio::null()
        },
    };

    run_process(
        spec,
        config,
        cancel,
        stdio,
        io.input(),
        Some(output),
        observer,
    )
    .await
}

async fn run_inherited_mode(
    spec: &ProcessSpec,
    config: &ProcessConfig,
    cancel: CancellationToken,
    io: &InheritedIo,
) -> AppResult<ProcessResult> {
    let inherited_config = inherited_config(config);
    let stdio = PipeStdio {
        stdin: stdin_stdio(&io.input),
        stdout: Stdio::inherit(),
        stderr: Stdio::inherit(),
    };
    run_process(
        spec,
        &inherited_config,
        cancel,
        stdio,
        &io.input,
        None,
        None,
    )
    .await
}

async fn run_process(
    spec: &ProcessSpec,
    config: &ProcessConfig,
    cancel: CancellationToken,
    stdio: PipeStdio,
    input: &InputPolicy,
    output: Option<&OutputPolicy>,
    observer: Option<OutputObserver>,
) -> AppResult<ProcessResult> {
    if spec.program.as_os_str().is_empty() {
        return Err(AppError::invalid_input("program", "must not be empty"));
    }

    let start = Instant::now();
    let stdout_observer = observer
        .as_ref()
        .and_then(|observer| observer.stdout_line.clone());
    let stderr_observer = observer
        .as_ref()
        .and_then(|observer| observer.stderr_line.clone());
    let stdout_bytes_observer = observer
        .as_ref()
        .and_then(|observer| observer.stdout_bytes.clone());
    let stderr_bytes_observer = observer
        .as_ref()
        .and_then(|observer| observer.stderr_bytes.clone());

    let mut cmd = TokioCommand::new(&spec.program);
    configure_command(&mut cmd, spec, config, stdio);

    debug!(program = %spec.program.display(), args = ?RedactedArgs::new(&spec.args, &config.arg_redaction), "spawning process");
    let mut child = cmd
        .spawn()
        .map_err(|error| spawn_error("failed to spawn process", error))?;

    // Detach the child's pipe handles before the child moves into the scope guard,
    // which then owns it for the rest of the run.
    let stdout_pipe = child.stdout.take();
    let stderr_pipe = child.stderr.take();
    let stdin_pipe = child.stdin.take();

    // Own the child and the spawned I/O tasks in a scope guard:
    // any early return below aborts the reader/stdin tasks
    // and best-effort kills the child rather than detaching them (a dropped Tokio `JoinHandle` is detached, which would leak the task and keep the child's pipes alive).
    let mut scope = ChildScope::new(child);

    let max_output_bytes = output.and_then(|output| output.max_output_bytes);
    let capture_stdout = output.is_some_and(|output| output.capture_stdout);
    let capture_stderr = output.is_some_and(|output| output.capture_stderr);
    let stdout_capture = shared_output();
    let stderr_capture = shared_output();
    let stdout_task = spawn_reader(
        stdout_pipe,
        stdout_capture.clone(),
        max_output_bytes,
        stdout_observer,
        stdout_bytes_observer,
        capture_stdout,
    );
    scope.register(&stdout_task);
    let stderr_task = spawn_reader(
        stderr_pipe,
        stderr_capture.clone(),
        max_output_bytes,
        stderr_observer,
        stderr_bytes_observer,
        capture_stderr,
    );
    scope.register(&stderr_task);

    let stdin_task = spawn_stdin_writer(stdin_pipe, input);
    scope.register(&stdin_task);

    let completion = wait_for_completion(scope.child_mut(), spec, config, cancel).await?;

    // The child has exited; drain the workers within the grace period.
    // A reader still blocked because a surviving descendant holds the pipe open is aborted rather than awaited forever,
    // and the partial bytes it captured are still recovered from the shared buffer.
    let grace = config.signal.grace_period;
    join_within(stdin_task, grace).await?;
    join_within(stdout_task, grace).await?;
    join_within(stderr_task, grace).await?;
    let stdout_output = captured(&stdout_capture);
    let stdout_bytes = stdout_output.bytes;
    let stdout_truncated = stdout_output.truncated;
    let stderr_output = captured(&stderr_capture);

    // The run completed normally: the tasks are joined and the child is reaped,
    // so hand ownership back instead of aborting/killing on drop.
    scope.disarm();

    let mut stderr_bytes = stderr_output.bytes;
    let mut stderr_truncated = stderr_output.truncated;
    if let Some(extra_stderr) = completion.synthetic_stderr {
        stderr_truncated |=
            append_line_bounded(&mut stderr_bytes, extra_stderr.as_bytes(), max_output_bytes);
    }

    let result = ProcessResult::completed(
        completion.exit_code,
        stdout_bytes,
        stderr_bytes,
        stdout_truncated,
        stderr_truncated,
        start.elapsed(),
        completion.timed_out,
        completion.cancelled,
    );

    debug!(
        exit_code = ?result.exit_code,
        duration = ?result.duration,
        timed_out = result.timed_out,
        "process completed"
    );

    Ok(result)
}
