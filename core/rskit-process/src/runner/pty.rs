//! PTY-backed execution: allocate a pseudoterminal, run the child attached to it,
//! and stream/capture the merged output.

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
    ProcessSpec, ProcessSupervisor, command::spawn_error,
};

use super::lifecycle::wait_for_completion;
use super::output::{captured, join_within, spawn_reader};
use super::redaction::RedactedArgs;
use super::scope::ChildScope;
use super::spawn::spawn_with_etxtbsy_retry;
use crate::capture::{append_line_bounded, shared_output};

pub(in crate::runner) async fn run_pty_mode(
    supervisor: &ProcessSupervisor,
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
    if !config.lifecycle.isolate_process_group {
        // Acquiring a controlling terminal requires the child to be a session leader,
        // so PTY setup unconditionally calls `setsid()` (a new session, hence a new process group).
        // `isolate_process_group = false` therefore cannot be honored here;
        // reject it rather than silently ignoring it.
        return Err(AppError::invalid_input(
            "process.lifecycle.isolate_process_group",
            "PTY mode always starts a new session (setsid) to own the terminal, so isolate_process_group cannot be disabled",
        ));
    }

    let start = Instant::now();
    let PtyPair { master, slave } = open_pty(io.size)?;

    // Wire the child's stdin/stdout/stderr to three independent handles onto the slave
    // so the spawned command owns them; the parent drops its own slave handle right after spawn
    // so only the child keeps the terminal open.
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
    let child = spawn_with_etxtbsy_retry(&mut cmd)
        .await
        .map_err(|error| spawn_error("failed to spawn process", error))?;
    let registration =
        supervisor.register_pid_with_group(child.id().unwrap_or_default(), config.lifecycle);
    // The child now holds its own dup'd stdio; the parent must not keep the slave open
    // or the master read would never observe EOF.
    drop(slave);

    // Own the child and the spawned I/O tasks in a scope guard so any early
    // return below — a stdin/reader setup failure, a wait error, or a task join
    // error — aborts the background tasks and best-effort kills the child rather
    // than detaching them. A dropped Tokio `JoinHandle` is detached, so an
    // un-guarded early return would leak the task and keep the PTY fds alive.
    let mut scope = ChildScope::new(child, registration, config.lifecycle);

    // Optional input writer: a second handle onto the master so writes
    // and reads proceed independently. Built before the reader takes ownership of `master`.
    let stdin_writer = match &io.input {
        InputPolicy::Bytes(_) => Some(master.try_clone().map_err(AppError::internal)?),
        InputPolicy::Closed | InputPolicy::Inherit => None,
    };
    let stdin_task = spawn_pty_stdin(stdin_writer, &io.input);
    scope.register(&stdin_task);

    // The child's merged stdout+stderr arrives on the master;
    // route it through the stdout observer callbacks and (optionally) capture it as stdout.
    // Because the two streams are merged here,
    // either capture flag opts the caller into retaining the combined output.
    let reader = PtyMaster::new(master).map_err(AppError::internal)?;
    let reader_capture = shared_output();
    let reader_task = spawn_reader(
        Some(reader),
        reader_capture.clone(),
        io.output.max_output_bytes,
        io.observer.stdout_line.clone(),
        io.observer.stdout_bytes.clone(),
        io.output.capture_stdout || io.output.capture_stderr,
    );
    scope.register(&reader_task);

    let completion = wait_for_completion(scope.child_mut(), spec, config, cancel).await?;

    // The child has exited; drain the workers within the grace period.
    // A reader still blocked because a surviving descendant holds the PTY open is aborted rather than awaited forever,
    // and the partial bytes it captured are still recovered from the shared buffer.
    let grace = config.lifecycle.grace_period;
    join_within(stdin_task, grace).await?;
    join_within(reader_task, grace).await?;
    let captured_output = captured(&reader_capture);

    // The run completed: the tasks are joined. If the child exited it is reaped,
    // so hand ownership back on disarm. If it deliberately survived its grace
    // period (`kill_after_grace = false`), relinquish the still-live child to its
    // owned target so a later shutdown or supervisor drop reaps it.
    if completion.survived {
        scope.relinquish().await;
    } else {
        scope.disarm();
    }

    // A PTY has no separate stderr stream;
    // the only "stderr" is any synthetic termination diagnostic from the lifecycle layer.
    let mut stderr_bytes = Vec::new();
    let mut stderr_truncated = false;
    if let Some(extra_stderr) = completion.synthetic_stderr {
        stderr_truncated |= append_line_bounded(
            &mut stderr_bytes,
            extra_stderr.as_bytes(),
            io.output.max_output_bytes,
        );
    }

    let result = ProcessResult::completed(
        completion.exit_code,
        captured_output.bytes,
        stderr_bytes,
        captured_output.truncated,
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

#[cfg(test)]
mod tests {
    use std::process::Stdio;

    use super::*;
    use crate::pty::PtySize;

    #[test]
    fn pty_mode_rejects_invalid_program_input_and_signal_policy() {
        let runtime = tokio::runtime::Runtime::new().unwrap();
        runtime.block_on(async {
            let supervisor = ProcessSupervisor::new(crate::LifecyclePolicy::default());
            let config = ProcessConfig::default();
            let io = PtyIo::default();
            let error = run_pty_mode(
                &supervisor,
                &ProcessSpec::new(""),
                &config,
                CancellationToken::new(),
                &io,
            )
            .await
            .unwrap_err();
            assert_eq!(error.code(), ErrorCode::InvalidInput);

            let error = run_pty_mode(
                &supervisor,
                &ProcessSpec::new("cat"),
                &config,
                CancellationToken::new(),
                &PtyIo::default().with_input(InputPolicy::Inherit),
            )
            .await
            .unwrap_err();
            assert_eq!(error.code(), ErrorCode::InvalidInput);

            let disabled_group = ProcessConfig::default().with_lifecycle_policy(
                crate::LifecyclePolicy::default().with_isolate_process_group(false),
            );
            let error = run_pty_mode(
                &supervisor,
                &ProcessSpec::new("cat"),
                &disabled_group,
                CancellationToken::new(),
                &PtyIo::default(),
            )
            .await
            .unwrap_err();
            assert_eq!(error.code(), ErrorCode::InvalidInput);
        });
    }

    #[test]
    fn configure_pty_command_applies_dir_env_and_empty_policy() {
        let mut command = TokioCommand::new("/bin/echo");
        let spec = ProcessSpec::new("ignored")
            .arg("hello")
            .dir(".")
            .env("RSKIT_PROCESS_TEST", "1")
            .empty_env();
        configure_pty_command(
            &mut command,
            &spec,
            Stdio::null(),
            Stdio::null(),
            Stdio::null(),
        );
        let _ = PtySize::default();
    }
}
