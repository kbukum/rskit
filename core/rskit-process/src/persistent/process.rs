use std::{
    process::{Child, ExitStatus},
    sync::{Arc, atomic::AtomicBool, atomic::Ordering},
    thread,
    time::{Duration, Instant},
};

use crate::{
    AppError, AppResult, ErrorCode, ProcessResult, SignalPolicy,
    process_group::{kill_target, terminate_target},
};

use super::{
    cancel::CancelThread,
    io::{Capture, ReaderThread, StdinThread, join_reader, join_stdin, take_capture},
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
    signal: SignalPolicy,
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
            let cancelled = self.cancelled.load(Ordering::SeqCst);
            return self
                .completed_result(status, false, cancelled)
                .map(ShutdownOutcome::AlreadyExited);
        }

        self.stop_cancel_thread()?;
        let pid = self.child.id();
        if !terminate_target(pid, terminate_group(self.signal)) {
            let _ = self.child.kill();
        }
        let status = wait_for_shutdown(
            &mut self.child,
            pid,
            self.signal,
            self.shutdown_grace_period,
        )?;
        self.stopped = true;
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
        join_stdin(self.stdin_thread.take())?;
        join_reader(self.stdout_thread.take())?;
        join_reader(self.stderr_thread.take())?;
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
}

impl Drop for PersistentProcess {
    fn drop(&mut self) {
        if !self.stopped {
            let _ = self.shutdown_inner();
        }
    }
}

#[allow(clippy::too_many_arguments)]
pub(in crate::persistent) fn new_process(
    child: Child,
    stdin_thread: StdinThread,
    stdout_thread: ReaderThread,
    stderr_thread: ReaderThread,
    cancel_thread: Option<CancelThread>,
    cancelled: Arc<AtomicBool>,
    stdout: Capture,
    stderr: Capture,
    start: Instant,
    signal: SignalPolicy,
    shutdown_grace_period: Duration,
) -> PersistentProcess {
    PersistentProcess {
        child,
        stdin_thread,
        stdout_thread,
        stderr_thread,
        cancel_thread,
        cancelled,
        stdout,
        stderr,
        start,
        signal,
        shutdown_grace_period,
        stopped: false,
    }
}

pub(in crate::persistent) fn cleanup_spawned_child(
    child: &mut Child,
    signal: SignalPolicy,
    grace_period: Duration,
) -> AppResult<()> {
    let pid = child.id();
    if !terminate_target(pid, terminate_group(signal)) {
        let _ = child.kill();
    }
    wait_for_shutdown(child, pid, signal, grace_period).map(|_| ())
}

fn wait_for_shutdown(
    child: &mut Child,
    pid: u32,
    signal: SignalPolicy,
    grace_period: Duration,
) -> AppResult<ExitStatus> {
    let deadline = Instant::now() + grace_period;
    loop {
        if let Some(status) = child.try_wait().map_err(AppError::internal)? {
            return Ok(status);
        }
        if Instant::now() >= deadline {
            if !kill_target(pid, terminate_group(signal)) {
                let _ = child.kill();
            }
            return child.wait().map_err(AppError::internal);
        }
        thread::sleep(Duration::from_millis(10));
    }
}

fn wait_for_exit(
    child: &mut Child,
    cancel_thread: &Option<CancelThread>,
    signal: SignalPolicy,
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
            let pid = child.id();
            if !terminate_target(pid, terminate_group(signal)) {
                let _ = child.kill();
            }
            return wait_for_shutdown(child, pid, signal, grace_period);
        }
        thread::sleep(Duration::from_millis(10));
    }
}

fn terminate_group(signal: SignalPolicy) -> bool {
    signal.create_process_group && signal.terminate_descendants
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
            SignalPolicy::default()
                .with_create_process_group(false)
                .with_terminate_descendants(false),
            Duration::from_millis(20),
        )
        .expect("cleanup should stop spawned child");
    }
}
