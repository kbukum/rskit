//! Async subprocess execution runtime.

use std::io::ErrorKind;
use std::process::Stdio;
use std::time::Instant;

use tokio::{process::ChildStdin, process::Command as TokioCommand, task::JoinHandle};
use tokio_util::sync::CancellationToken;
use tracing::debug;

use crate::{
    AppError, AppResult, CapturedIo, ErrorCode, InheritedIo, InputPolicy, ObservedIo, OutputPolicy,
    ProcessConfig, ProcessIo, ProcessResult, ProcessSpec,
};

mod lifecycle;
mod observer;
mod output;
mod spawn;

pub use observer::OutputObserver;

use lifecycle::wait_for_completion;
use output::{append_bounded_stderr, collect_reader, spawn_reader};
use spawn::configure_command;

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
    }
}

trait PipeIo {
    fn input(&self) -> &InputPolicy;
    fn output(&self) -> &OutputPolicy;
}

impl PipeIo for CapturedIo {
    fn input(&self) -> &InputPolicy {
        &self.input
    }

    fn output(&self) -> &OutputPolicy {
        &self.output
    }
}

impl PipeIo for ObservedIo {
    fn input(&self) -> &InputPolicy {
        &self.input
    }

    fn output(&self) -> &OutputPolicy {
        &self.output
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

struct PipeStdio {
    stdin: Stdio,
    stdout: Stdio,
    stderr: Stdio,
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
    configure_command(
        &mut cmd,
        spec,
        config,
        stdio.stdin,
        stdio.stdout,
        stdio.stderr,
    );

    debug!(program = %spec.program.display(), args = ?spec.args, "spawning process");
    let mut child = cmd.spawn().map_err(|error| {
        AppError::new(
            ErrorCode::Internal,
            format!("failed to spawn process: {error}"),
        )
    })?;

    let max_output_bytes = output.and_then(|output| output.max_output_bytes);
    let capture_stdout = output.is_some_and(|output| output.capture_stdout);
    let capture_stderr = output.is_some_and(|output| output.capture_stderr);
    let stdout_task = spawn_reader(
        child.stdout.take(),
        max_output_bytes,
        stdout_observer,
        stdout_bytes_observer,
        capture_stdout,
    );
    let stderr_task = spawn_reader(
        child.stderr.take(),
        max_output_bytes,
        stderr_observer,
        stderr_bytes_observer,
        capture_stderr,
    );

    let stdin_task = spawn_stdin_writer(child.stdin.take(), input);

    let completion = wait_for_completion(&mut child, spec, config, cancel).await?;
    collect_stdin(stdin_task).await?;

    let stdout_output = collect_reader(stdout_task).await?;
    let stdout_bytes = stdout_output.bytes;
    let stdout_truncated = stdout_output.truncated;
    let stderr_output = collect_reader(stderr_task).await?;
    let mut stderr_bytes = stderr_output.bytes;
    let mut stderr_truncated = stderr_output.truncated;
    if let Some(extra_stderr) = completion.synthetic_stderr {
        stderr_truncated |=
            append_bounded_stderr(&mut stderr_bytes, extra_stderr.as_bytes(), max_output_bytes);
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

fn spawn_stdin_writer(
    stdin: Option<ChildStdin>,
    input: &InputPolicy,
) -> Option<JoinHandle<AppResult<()>>> {
    let InputPolicy::Bytes(bytes) = input else {
        return None;
    };
    let mut stdin = stdin?;
    let bytes = bytes.clone();
    Some(tokio::spawn(async move {
        use tokio::io::AsyncWriteExt;

        match stdin.write_all(&bytes).await {
            Ok(()) => Ok(()),
            Err(error) if error.kind() == ErrorKind::BrokenPipe => Ok(()),
            Err(error) => Err(AppError::new(
                ErrorCode::Internal,
                format!("failed to write to stdin: {error}"),
            )),
        }
    }))
}

async fn collect_stdin(task: Option<JoinHandle<AppResult<()>>>) -> AppResult<()> {
    match task {
        Some(task) => task.await.map_err(AppError::internal)?,
        None => Ok(()),
    }
}
