use std::{
    process::{Child, ExitStatus},
    sync::{Arc, atomic::AtomicBool, atomic::Ordering},
    thread,
    time::{Duration, Instant},
};

use crate::worker::join_within;
use crate::{
    AppError, AppResult, ErrorCode, LifecyclePolicy, ProcessResult, ProcessSupervisor,
    RegistrationGuard, supervisor::terminate_and_reap,
};

use super::{
    cancel::CancelThread,
    io::{Capture, ReaderThread, StdinThread, take_capture},
};

/// Outcome of requesting persistent process shutdown.
#[derive(Debug, Clone)]
#[non_exhaustive]
pub enum ShutdownOutcome {
    /// The process had already exited before shutdown was requested.
    AlreadyExited(ProcessResult),
    /// The process was stopped by the shutdown request.
    Stopped(ProcessResult),
}

/// Running persistent process.
#[derive(Debug)]
pub struct PersistentProcess {
    // Keep field order stable: drop order is part of the lifecycle safety story.
    child: Child,
    stdin_thread: StdinThread,
    stdout_thread: ReaderThread,
    stderr_thread: ReaderThread,
    cancel_thread: Option<CancelThread>,
    cancelled: Arc<AtomicBool>,
    stdout: Capture,
    stderr: Capture,
    start: Instant,
    signal: LifecyclePolicy,
    registration: Option<RegistrationGuard>,
    _supervisor: Option<ProcessSupervisor>,
    shutdown_grace_period: Duration,
    stopped: bool,
}

impl PersistentProcess {
    /// Wait for the persistent process to exit naturally.
    pub fn wait(mut self) -> AppResult<ProcessResult> {
        self.wait_inner()
    }

    /// Gracefully stop the persistent process.
    pub fn shutdown(mut self) -> AppResult<ShutdownOutcome> {
        self.shutdown_inner()
    }

    fn wait_inner(&mut self) -> AppResult<ProcessResult> {
        if self.stopped {
            return Err(AppError::new(
                ErrorCode::Conflict,
                "persistent process already stopped",
            ));
        }
        let status = wait_for_exit(
            &mut self.child,
            &self.cancel_thread,
            self.signal,
            self.shutdown_grace_period,
        )?;
        self.stopped = true;
        self.stop_cancel_thread()?;
        self.unregister();
        let cancelled = self.cancelled.load(Ordering::SeqCst);
        self.completed_result(status, false, cancelled)
    }

    pub(in crate::persistent) fn shutdown_inner(&mut self) -> AppResult<ShutdownOutcome> {
        if self.stopped {
            return Err(AppError::new(
                ErrorCode::Conflict,
                "persistent process already stopped",
            ));
        }
        if let Some(status) = self.child.try_wait().map_err(AppError::internal)? {
            self.stopped = true;
            self.stop_cancel_thread()?;
            self.unregister();
            let cancelled = self.cancelled.load(Ordering::SeqCst);
            return self
                .completed_result(status, false, cancelled)
                .map(ShutdownOutcome::AlreadyExited);
        }

        self.stop_cancel_thread()?;
        let (status, _escalated) =
            terminate_and_reap(&mut self.child, self.signal, self.shutdown_grace_period)?;
        self.stopped = true;
        self.unregister();
        let cancelled = self.cancelled.load(Ordering::SeqCst);
        self.completed_result(status, false, cancelled)
            .map(ShutdownOutcome::Stopped)
    }

    fn completed_result(
        &mut self,
        status: ExitStatus,
        timed_out: bool,
        cancelled: bool,
    ) -> AppResult<ProcessResult> {
        let grace = self.shutdown_grace_period;
        join_within(self.stdin_thread.take(), grace)?;
        join_within(self.stdout_thread.take(), grace)?;
        join_within(self.stderr_thread.take(), grace)?;
        let stdout = take_capture(&self.stdout);
        let stderr = take_capture(&self.stderr);
        Ok(ProcessResult::completed(
            status.code(),
            stdout.bytes,
            stderr.bytes,
            stdout.truncated,
            stderr.truncated,
            self.start.elapsed(),
            timed_out,
            cancelled,
        ))
    }

    fn stop_cancel_thread(&mut self) -> AppResult<()> {
        if let Some(thread) = self.cancel_thread.take() {
            thread.stop()
        } else {
            Ok(())
        }
    }

    fn unregister(&mut self) {
        if let Some(registration) = self.registration.take() {
            registration.unregister();
        }
    }
}

impl Drop for PersistentProcess {
    fn drop(&mut self) {
        if !self.stopped {
            let _ = self.shutdown_inner();
        }
    }
}

/// The live handles and captured state produced by spawning a persistent process.
///
/// These pieces are assembled into a [`PersistentProcess`] once it satisfies its readiness policy,
/// or cleaned up through the normal lifecycle if it fails one.
pub(in crate::persistent) struct SpawnedProcess {
    pub(in crate::persistent) child: Child,
    pub(in crate::persistent) stdin_thread: StdinThread,
    pub(in crate::persistent) stdout_thread: ReaderThread,
    pub(in crate::persistent) stderr_thread: ReaderThread,
    pub(in crate::persistent) cancel_thread: Option<CancelThread>,
    pub(in crate::persistent) cancelled: Arc<AtomicBool>,
    pub(in crate::persistent) stdout: Capture,
    pub(in crate::persistent) stderr: Capture,
    pub(in crate::persistent) start: Instant,
    pub(in crate::persistent) registration: RegistrationGuard,
    pub(in crate::persistent) supervisor: Option<ProcessSupervisor>,
}

pub(in crate::persistent) fn new_process(
    spawned: SpawnedProcess,
    signal: LifecyclePolicy,
    shutdown_grace_period: Duration,
) -> PersistentProcess {
    PersistentProcess {
        child: spawned.child,
        stdin_thread: spawned.stdin_thread,
        stdout_thread: spawned.stdout_thread,
        stderr_thread: spawned.stderr_thread,
        cancel_thread: spawned.cancel_thread,
        cancelled: spawned.cancelled,
        stdout: spawned.stdout,
        stderr: spawned.stderr,
        start: spawned.start,
        signal,
        registration: Some(spawned.registration),
        _supervisor: spawned.supervisor,
        shutdown_grace_period,
        stopped: false,
    }
}

pub(in crate::persistent) fn cleanup_spawned_child(
    child: &mut Child,
    signal: LifecyclePolicy,
    grace_period: Duration,
) -> AppResult<()> {
    terminate_and_reap(child, signal, grace_period).map(|_| ())
}

fn wait_for_exit(
    child: &mut Child,
    cancel_thread: &Option<CancelThread>,
    signal: LifecyclePolicy,
    grace_period: Duration,
) -> AppResult<ExitStatus> {
    loop {
        if let Some(status) = child.try_wait().map_err(AppError::internal)? {
            return Ok(status);
        }
        if cancel_thread
            .as_ref()
            .is_some_and(CancelThread::is_cancel_requested)
        {
            return terminate_and_reap(child, signal, grace_period).map(|(status, _)| status);
        }
        thread::sleep(Duration::from_millis(10));
    }
}

#[cfg(all(test, unix))]
mod tests {
    use std::time::Duration;

    use tokio_util::sync::CancellationToken;

    use super::*;
    use crate::{
        ErrorCode, PersistentConfig, PersistentReadiness, ProcessConfig, ProcessSpec,
        start_persistent_with_cancel,
    };

    fn start_ready_process(command: &str) -> PersistentProcess {
        let spec = ProcessSpec::new("/bin/sh").arg("-c").arg(command);
        let config = PersistentConfig::default()
            .with_readiness(PersistentReadiness::OutputContains("ready".to_string()))
            .with_readiness_timeout(Duration::from_secs(2))
            .with_shutdown_grace_period(Duration::from_millis(50));

        start_persistent_with_cancel(
            &spec,
            &ProcessConfig::default(),
            &config,
            CancellationToken::new(),
        )
        .expect("persistent process starts")
        .process
    }

    #[test]
    fn wait_inner_rejects_reuse_after_shutdown() {
        let mut process = start_ready_process("printf ready; while :; do sleep 1; done");
        let _ = process.shutdown_inner().expect("initial shutdown succeeds");

        let error = process
            .wait_inner()
            .expect_err("wait after shutdown should fail");

        assert_eq!(error.code(), ErrorCode::Conflict);
    }

    #[test]
    fn shutdown_inner_rejects_reuse_after_wait() {
        let mut process = start_ready_process("printf ready; exit 0");
        let _ = process.wait_inner().expect("initial wait succeeds");

        let error = process
            .shutdown_inner()
            .expect_err("shutdown after wait should fail");

        assert_eq!(error.code(), ErrorCode::Conflict);
    }

    #[test]
    fn shutdown_inner_handles_missing_cancel_thread() {
        let mut process = start_ready_process("printf ready; while :; do sleep 1; done");
        process.cancel_thread = None;

        let outcome = process
            .shutdown_inner()
            .expect("shutdown should work without cancel thread handle");

        assert!(matches!(outcome, ShutdownOutcome::Stopped(_)));
    }

    #[test]
    fn cleanup_spawned_child_terminates_process() {
        let mut child = std::process::Command::new("/bin/sh")
            .arg("-c")
            .arg("while :; do sleep 1; done")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("child starts");

        cleanup_spawned_child(
            &mut child,
            LifecyclePolicy::default()
                .with_isolate_process_group(false)
                .with_terminate_descendants(false),
            Duration::from_millis(20),
        )
        .expect("cleanup should stop spawned child");
    }
}
