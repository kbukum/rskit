use std::{
    process::{Child, Command as StdCommand, Stdio},
    sync::{Arc, atomic::AtomicBool, mpsc},
    time::Instant,
};

use tokio_util::sync::CancellationToken;

use crate::{
    AppError, AppResult, EnvPolicy, InputPolicy, LifecyclePolicy, ProcessConfig, ProcessIo,
    ProcessSpec, ProcessSupervisor, process_group::isolate,
};

use super::cancel::spawn_cancel_thread;
use super::config::PersistentConfig;
use super::config::PersistentReadiness::{Command as CommandReadiness, OutputContains, Started};
use super::error::{PersistentStartErrorKind, persistent_start_error};
use super::io::{new_capture, spawn_output_readers, spawn_stdin_writer, take_capture};
use super::process::{PersistentProcess, SpawnedProcess, cleanup_spawned_child, new_process};
use super::readiness::{
    readiness_wait_error, run_readiness_command, validate_readiness, wait_for_readiness,
};
use super::types::{PersistentRun, PersistentStartup};

/// Start a persistent process and wait for its readiness policy.
///
/// Spawns through a throwaway per-call [`ProcessSupervisor`] that the returned
/// process owns for its lifetime, so drop reaps the child. Callers that want a
/// shared supervisor to reap the process on shutdown use
/// [`start_persistent_supervised`].
pub fn start_persistent_with_cancel(
    spec: &ProcessSpec,
    process_config: &ProcessConfig,
    persistent_config: &PersistentConfig,
    cancel: CancellationToken,
) -> AppResult<PersistentRun> {
    start_persistent_impl(spec, process_config, persistent_config, cancel, None)
}

/// Start a persistent process, registering it with the injected `supervisor`.
///
/// Identical to [`start_persistent_with_cancel`] except the shared `supervisor`
/// owns the registration, so a process-level [`ProcessSupervisor::shutdown`]
/// reaps the persistent group even while it is still running. The returned
/// process holds no supervisor of its own; its registration guard unregisters on
/// shutdown, so a backstop over an already-stopped process is a clean no-op.
pub fn start_persistent_supervised(
    supervisor: &ProcessSupervisor,
    spec: &ProcessSpec,
    process_config: &ProcessConfig,
    persistent_config: &PersistentConfig,
    cancel: CancellationToken,
) -> AppResult<PersistentRun> {
    start_persistent_impl(
        spec,
        process_config,
        persistent_config,
        cancel,
        Some(supervisor),
    )
}

fn start_persistent_impl(
    spec: &ProcessSpec,
    process_config: &ProcessConfig,
    persistent_config: &PersistentConfig,
    cancel: CancellationToken,
    injected: Option<&ProcessSupervisor>,
) -> AppResult<PersistentRun> {
    if spec.program.as_os_str().is_empty() {
        return Err(AppError::invalid_input("program", "must not be empty"));
    }
    if cancel.is_cancelled() {
        return Err(AppError::cancelled("persistent process startup"));
    }
    validate_readiness(&persistent_config.readiness)?;

    let start = Instant::now();
    let input = process_input(process_config)?;
    let mut child = spawn_child(spec, process_config, input)?;
    // An injected supervisor owns the registration (shared registry, reaped by a
    // process-level shutdown). Otherwise the process owns a local supervisor for
    // its lifetime so drop reaps the child.
    let (supervisor, registration) = match injected {
        Some(supervisor) => (
            None,
            supervisor
                .register_pid_with_group(child.id(), process_config.lifecycle.targets_group()),
        ),
        None => {
            let local = ProcessSupervisor::new(process_config.lifecycle);
            let registration =
                local.register_pid_with_group(child.id(), process_config.lifecycle.targets_group());
            (Some(local), registration)
        }
    };
    let stdout = new_capture();
    let stderr = new_capture();
    let cancelled = Arc::new(AtomicBool::new(false));
    let cancel_thread = match spawn_cancel_thread(
        child.id(),
        cancel.clone(),
        Arc::clone(&cancelled),
        process_config.lifecycle,
        persistent_config.shutdown_grace_period,
    ) {
        Ok(thread) => Some(thread),
        Err(error) => {
            let _ = cleanup_spawned_child(
                &mut child,
                process_config.lifecycle,
                persistent_config.shutdown_grace_period,
            );
            return Err(error);
        }
    };
    let (ready_tx, ready_rx) = mpsc::channel();
    let (stdout_thread, stderr_thread) =
        spawn_output_readers(&mut child, &stdout, &stderr, &ready_tx, persistent_config);
    let stdin_thread = spawn_stdin_writer(&mut child, predefined_stdin(input));

    let mut spawned = SpawnedProcess {
        child,
        stdin_thread,
        stdout_thread,
        stderr_thread,
        cancel_thread,
        cancelled,
        stdout,
        stderr,
        start,
        registration,
        supervisor,
    };

    match &persistent_config.readiness {
        Started => {
            let _ = ready_tx.send(());
        }
        CommandReadiness(command) => {
            if let Err(error) = run_readiness_command(
                command,
                process_config,
                persistent_config.readiness_timeout,
                persistent_config.shutdown_grace_period,
                cancel.clone(),
            ) {
                let mut process =
                    persistent_process(spawned, process_config.lifecycle, persistent_config);
                // Reuse the normal lifecycle cleanup
                // so partially-started processes are terminated the same way as explicit shutdown.
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
        &spawned.cancelled,
    ) {
        let readiness_error = readiness_wait_error(&mut spawned.child, error, &spawned.cancelled)?;
        let mut process = persistent_process(spawned, process_config.lifecycle, persistent_config);
        // Reuse the normal lifecycle cleanup so readiness failures do not leak the child.
        let _ = process.shutdown_inner();
        return Err(readiness_error);
    }

    let stdout_startup = take_capture(&spawned.stdout);
    let stderr_startup = take_capture(&spawned.stderr);
    let process = persistent_process(spawned, process_config.lifecycle, persistent_config);

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

fn process_input(config: &ProcessConfig) -> AppResult<&InputPolicy> {
    match &config.io {
        ProcessIo::Captured(io)
            if matches!(io.input, InputPolicy::Closed | InputPolicy::Bytes(_)) =>
        {
            Ok(&io.input)
        }
        ProcessIo::Captured(_) => Err(AppError::invalid_input(
            "process.io.input",
            "persistent processes support only closed stdin or predefined stdin bytes",
        )),
        ProcessIo::Inherited(_) => Err(AppError::invalid_input(
            "process.io",
            "persistent processes use PersistentOutput for output handling; inherited mode is not supported",
        )),
        ProcessIo::Observed(_) => Err(AppError::invalid_input(
            "process.io",
            "persistent processes use PersistentOutput for observation; observed mode is not supported",
        )),
        #[cfg(unix)]
        ProcessIo::Pty(_) => Err(AppError::invalid_input(
            "process.io",
            "persistent processes use PersistentOutput for output handling; pty mode is not supported",
        )),
    }
}

fn predefined_stdin(input: &InputPolicy) -> Option<Vec<u8>> {
    match input {
        InputPolicy::Bytes(bytes) => Some(bytes.clone()),
        InputPolicy::Closed | InputPolicy::Inherit => None,
    }
}

fn spawn_child(
    spec: &ProcessSpec,
    config: &ProcessConfig,
    input: &InputPolicy,
) -> AppResult<Child> {
    let mut cmd = StdCommand::new(&spec.program);
    cmd.args(&spec.args)
        .stdin(if matches!(input, InputPolicy::Bytes(_)) {
            Stdio::piped()
        } else {
            Stdio::null()
        })
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

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
        isolate(&mut cmd);
    }

    cmd.spawn().map_err(|error| {
        let message = format!("failed to spawn persistent process: {error}");
        let classified = AppError::from(error);
        persistent_start_error(
            PersistentStartErrorKind::SpawnFailed,
            classified.code(),
            message,
        )
        .with_cause(classified)
    })
}

fn persistent_process(
    spawned: SpawnedProcess,
    signal: LifecyclePolicy,
    config: &PersistentConfig,
) -> PersistentProcess {
    new_process(spawned, signal, config.shutdown_grace_period)
}
