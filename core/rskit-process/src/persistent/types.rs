use std::time::Duration;

use super::PersistentProcess;

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
