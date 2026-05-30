//! Persistent subprocess lifecycle support.

use std::{
    process::{Child, Command as StdCommand, Stdio},
    sync::{Arc, atomic::AtomicBool, mpsc},
    time::{Duration, Instant},
};

use parking_lot::Mutex;
use tokio_util::sync::CancellationToken;

use crate::{AppError, AppResult, Command, ErrorCode, ProcessConfig, process_group::isolate};

mod cancel;
mod config;
mod io;
mod process;
mod readiness;

#[cfg(all(test, unix))]
mod tests;

pub use config::{PersistentConfig, PersistentOutput, PersistentOutputStream, PersistentReadiness};
pub use process::{PersistentProcess, ShutdownOutcome};

use cancel::spawn_cancel_thread;
use config::PersistentReadiness::{Command as CommandReadiness, OutputContains, Started};
use io::{
    CapturedOutput, ReaderThread, StdinThread, spawn_output_readers, spawn_stdin_writer,
    take_capture,
};
use process::{cleanup_spawned_child, new_process};
use readiness::{
    readiness_wait_error, run_readiness_command, validate_readiness, wait_for_readiness,
};

/// Captured output retained while waiting for persistent readiness.
#[derive(Debug, Clone)]
pub struct PersistentStartup {
    /// Captured stdout at the moment readiness completed.
    pub stdout: String,
    /// Captured stdout bytes at the moment readiness completed.
    pub stdout_bytes: Vec<u8>,
    /// Captured stderr at the moment readiness completed.
    pub stderr: String,
    /// Captured stderr bytes at the moment readiness completed.
    pub stderr_bytes: Vec<u8>,
    /// Whether stdout capture exceeded the configured limit before readiness.
    pub stdout_truncated: bool,
    /// Whether stderr capture exceeded the configured limit before readiness.
    pub stderr_truncated: bool,
    /// Time elapsed from spawn until readiness completed.
    pub duration: Duration,
}

/// Result of starting a persistent process.
#[derive(Debug)]
pub struct PersistentRun {
    /// Startup output captured while waiting for readiness.
    pub startup: PersistentStartup,
    /// Running persistent process handle.
    pub process: PersistentProcess,
}

/// Start a persistent process and wait for its readiness policy.
pub fn start_persistent_with_cancel(
    command: &Command,
    process_config: &ProcessConfig,
    persistent_config: &PersistentConfig,
    cancel: CancellationToken,
) -> AppResult<PersistentRun> {
    if command.program.as_os_str().is_empty() {
        return Err(AppError::invalid_input("program", "must not be empty"));
    }
    if cancel.is_cancelled() {
        return Err(AppError::cancelled("persistent process startup"));
    }
    validate_readiness(&persistent_config.readiness)?;

    let start = Instant::now();
    let mut child = spawn_child(command, process_config)?;
    let stdout = Arc::new(Mutex::new(CapturedOutput::default()));
    let stderr = Arc::new(Mutex::new(CapturedOutput::default()));
    let cancelled = Arc::new(AtomicBool::new(false));
    let cancel_thread = match spawn_cancel_thread(
        child.id(),
        cancel.clone(),
        Arc::clone(&cancelled),
        persistent_config.shutdown_grace_period,
    ) {
        Ok(thread) => Some(thread),
        Err(error) => {
            let _ = cleanup_spawned_child(&mut child, persistent_config.shutdown_grace_period);
            return Err(error);
        }
    };
    let (ready_tx, ready_rx) = mpsc::channel();
    let (stdout_thread, stderr_thread) = spawn_output_readers(
        &mut child,
        &stdout,
        &stderr,
        &ready_tx,
        &persistent_config.readiness,
        persistent_config.output,
        persistent_config.max_capture_bytes,
    );
    let stdin_thread = spawn_stdin_writer(&mut child, command.stdin.clone());

    match &persistent_config.readiness {
        Started => {
            let _ = ready_tx.send(());
        }
        CommandReadiness(command) => {
            if let Err(error) = run_readiness_command(
                command,
                process_config,
                persistent_config.readiness_timeout,
                cancel.clone(),
            ) {
                let mut process = persistent_process(
                    child,
                    stdin_thread,
                    stdout_thread,
                    stderr_thread,
                    cancel_thread,
                    cancelled,
                    stdout,
                    stderr,
                    start,
                    persistent_config,
                );
                // Reuse the normal lifecycle cleanup so partially-started processes
                // are terminated the same way as explicit shutdown.
                let _ = process.shutdown_inner();
                return Err(error);
            }
            let _ = ready_tx.send(());
        }
        OutputContains(_) => {}
    }
    drop(ready_tx);

    if let Err(error) = wait_for_readiness(
        &ready_rx,
        persistent_config.readiness_timeout,
        &cancel,
        &cancelled,
    ) {
        let readiness_error = readiness_wait_error(&mut child, error, &cancelled)?;
        let mut process = persistent_process(
            child,
            stdin_thread,
            stdout_thread,
            stderr_thread,
            cancel_thread,
            cancelled,
            stdout,
            stderr,
            start,
            persistent_config,
        );
        // Reuse the normal lifecycle cleanup so readiness failures do not leak the child.
        let _ = process.shutdown_inner();
        return Err(readiness_error);
    }

    let stdout_startup = take_capture(&stdout);
    let stderr_startup = take_capture(&stderr);
    let process = persistent_process(
        child,
        stdin_thread,
        stdout_thread,
        stderr_thread,
        cancel_thread,
        cancelled,
        stdout,
        stderr,
        start,
        persistent_config,
    );

    Ok(PersistentRun {
        startup: PersistentStartup {
            stdout: String::from_utf8_lossy(&stdout_startup.bytes).into_owned(),
            stdout_bytes: stdout_startup.bytes,
            stderr: String::from_utf8_lossy(&stderr_startup.bytes).into_owned(),
            stderr_bytes: stderr_startup.bytes,
            stdout_truncated: stdout_startup.truncated,
            stderr_truncated: stderr_startup.truncated,
            duration: start.elapsed(),
        },
        process,
    })
}

fn spawn_child(command: &Command, config: &ProcessConfig) -> AppResult<Child> {
    let mut cmd = StdCommand::new(&command.program);
    cmd.args(&command.args)
        .stdin(if command.stdin.is_some() {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    if let Some(dir) = &command.dir {
        cmd.current_dir(dir);
    }
    if command.scrub_env || !config.inherit_env {
        cmd.env_clear();
    }
    for (key, value) in &command.env {
        cmd.env(key, value);
    }
    isolate(&mut cmd);

    cmd.spawn().map_err(|error| {
        AppError::new(
            ErrorCode::Internal,
            format!("failed to spawn persistent process: {error}"),
        )
    })
}

#[allow(clippy::too_many_arguments)]
fn persistent_process(
    child: Child,
    stdin_thread: StdinThread,
    stdout_thread: ReaderThread,
    stderr_thread: ReaderThread,
    cancel_thread: Option<cancel::CancelThread>,
    cancelled: Arc<AtomicBool>,
    stdout: io::Capture,
    stderr: io::Capture,
    start: Instant,
    config: &PersistentConfig,
) -> PersistentProcess {
    new_process(
        child,
        stdin_thread,
        stdout_thread,
        stderr_thread,
        cancel_thread,
        cancelled,
        stdout,
        stderr,
        start,
        config.shutdown_grace_period,
    )
}
