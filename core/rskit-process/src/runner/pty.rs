//! PTY-backed execution: allocate a pseudoterminal, run the child attached to
//! it, and stream/capture the merged output.

use std::io::ErrorKind;
use std::os::unix::io::OwnedFd;
use std::process::Stdio;
use std::time::Instant;

use tokio::io::AsyncWriteExt;
use tokio::process::Command as TokioCommand;
use tokio::task::JoinHandle;
use tokio_util::sync::CancellationToken;
use tracing::debug;

use crate::pty::{PtyIo, PtyMaster, PtyPair, install_controlling_tty, open_pty};
use crate::{
    AppError, AppResult, EnvPolicy, ErrorCode, InputPolicy, ProcessConfig, ProcessResult,
    ProcessSpec,
};

use super::lifecycle::wait_for_completion;
use super::output::{append_bounded_stderr, collect_reader, spawn_reader};
use super::redaction::RedactedArgs;

pub(in crate::runner) async fn run_pty_mode(
    spec: &ProcessSpec,
    config: &ProcessConfig,
    cancel: CancellationToken,
    io: &PtyIo,
) -> AppResult<ProcessResult> {
    if spec.program.as_os_str().is_empty() {
        return Err(AppError::invalid_input("program", "must not be empty"));
    }
    if matches!(io.input, InputPolicy::Inherit) {
        return Err(AppError::invalid_input(
            "process.io.input",
            "inherited stdin requires inherited I/O mode; PTY mode owns the child's terminal",
        ));
    }
    if !config.signal.create_process_group {
        // Acquiring a controlling terminal requires the child to be a session
        // leader, so PTY setup unconditionally calls `setsid()` (a new session,
        // hence a new process group). `create_process_group = false` therefore
        // cannot be honored here; reject it rather than silently ignoring it.
        return Err(AppError::invalid_input(
            "process.signal.create_process_group",
            "PTY mode always starts a new session (setsid) to own the terminal, so create_process_group cannot be disabled",
        ));
    }

    let start = Instant::now();
    let PtyPair { master, slave } = open_pty(io.size)?;

    // Wire the child's stdin/stdout/stderr to three independent handles onto the
    // slave so the spawned command owns them; the parent drops its own slave
    // handle right after spawn so only the child keeps the terminal open.
    let child_stdin = slave_stdio(&slave)?;
    let child_stdout = slave_stdio(&slave)?;
    let child_stderr = slave_stdio(&slave)?;

    let mut cmd = TokioCommand::new(&spec.program);
    configure_pty_command(&mut cmd, spec, child_stdin, child_stdout, child_stderr);
    install_controlling_tty(&mut cmd);

    debug!(
        program = %spec.program.display(),
        args = ?RedactedArgs::new(&spec.args, &config.arg_redaction),
        "spawning process on pseudoterminal"
    );
    let mut child = cmd.spawn().map_err(|error| {
        AppError::new(
            ErrorCode::Internal,
            format!("failed to spawn process: {error}"),
        )
    })?;
    // The child now holds its own dup'd stdio; the parent must not keep the
    // slave open or the master read would never observe EOF.
    drop(slave);

    // Optional input writer: a second handle onto the master so writes and reads
    // proceed independently. Built before the reader takes ownership of `master`.
    let stdin_writer = match &io.input {
        InputPolicy::Bytes(_) => Some(master.try_clone().map_err(AppError::internal)?),
        InputPolicy::Closed | InputPolicy::Inherit => None,
    };
    let stdin_task = spawn_pty_stdin(stdin_writer, &io.input);

    // The child's merged stdout+stderr arrives on the master; route it through
    // the stdout observer callbacks and (optionally) capture it as stdout.
    // Because the two streams are merged here, either capture flag opts the
    // caller into retaining the combined output.
    let reader = PtyMaster::new(master).map_err(AppError::internal)?;
    let reader_task = spawn_reader(
        Some(reader),
        io.output.max_output_bytes,
        io.observer.stdout_line.clone(),
        io.observer.stdout_bytes.clone(),
        io.output.capture_stdout || io.output.capture_stderr,
    );

    let completion = wait_for_completion(&mut child, spec, config, cancel).await?;
    collect_stdin(stdin_task).await?;

    let captured = collect_reader(reader_task).await?;

    // A PTY has no separate stderr stream; the only "stderr" is any synthetic
    // termination diagnostic from the lifecycle layer.
    let mut stderr_bytes = Vec::new();
    let mut stderr_truncated = false;
    if let Some(extra_stderr) = completion.synthetic_stderr {
        stderr_truncated |= append_bounded_stderr(
            &mut stderr_bytes,
            extra_stderr.as_bytes(),
            io.output.max_output_bytes,
        );
    }

    let result = ProcessResult::completed(
        completion.exit_code,
        captured.bytes,
        stderr_bytes,
        captured.truncated,
        stderr_truncated,
        start.elapsed(),
        completion.timed_out,
        completion.cancelled,
    );

    debug!(
        exit_code = ?result.exit_code,
        duration = ?result.duration,
        timed_out = result.timed_out,
        "pty process completed"
    );

    Ok(result)
}

fn slave_stdio(slave: &OwnedFd) -> AppResult<Stdio> {
    let cloned = slave.try_clone().map_err(AppError::internal)?;
    Ok(Stdio::from(cloned))
}

fn configure_pty_command(
    cmd: &mut TokioCommand,
    spec: &ProcessSpec,
    stdin: Stdio,
    stdout: Stdio,
    stderr: Stdio,
) {
    cmd.args(&spec.args)
        .stdin(stdin)
        .stdout(stdout)
        .stderr(stderr);

    if let Some(dir) = &spec.dir {
        cmd.current_dir(dir);
    }

    if matches!(spec.env_policy, EnvPolicy::Empty) {
        cmd.env_clear();
    }
    for (key, value) in &spec.env {
        cmd.env(key, value);
    }
    // No `setpgid` isolation here: the controlling-tty hook calls `setsid`,
    // which already places the child in its own session and process group.
}

fn spawn_pty_stdin(
    writer: Option<OwnedFd>,
    input: &InputPolicy,
) -> Option<JoinHandle<AppResult<()>>> {
    let InputPolicy::Bytes(bytes) = input else {
        return None;
    };
    let writer = writer?;
    let bytes = bytes.clone();
    Some(tokio::spawn(async move {
        let mut master = PtyMaster::new(writer).map_err(AppError::internal)?;
        match master.write_all(&bytes).await {
            Ok(()) => Ok(()),
            // The child may exit before draining its input; that is not an error.
            Err(error)
                if error.kind() == ErrorKind::BrokenPipe
                    || error.raw_os_error() == Some(libc::EIO) =>
            {
                Ok(())
            }
            Err(error) => Err(AppError::new(
                ErrorCode::Internal,
                format!("failed to write to pty stdin: {error}"),
            )),
        }
        // Dropping `master` closes the input handle.
    }))
}

async fn collect_stdin(task: Option<JoinHandle<AppResult<()>>>) -> AppResult<()> {
    match task {
        Some(task) => task.await.map_err(AppError::internal)?,
        None => Ok(()),
    }
}
