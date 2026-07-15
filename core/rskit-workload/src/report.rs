//! Runtime state and result reports returned by a [`crate::Manager`].

use std::collections::HashMap;

use chrono::{DateTime, Utc};

use crate::state::WorkloadState;

/// Returned after a successful deployment.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeployResult {
    /// Provider-assigned identifier (container ID or pod/job name).
    pub id: String,
    /// Human-readable name.
    pub name: String,
    /// Initial state.
    pub state: WorkloadState,
}

/// Returned when a workload exits.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WaitResult {
    /// Process exit code.
    pub status_code: i64,
    /// Failure reason, when the workload exited abnormally.
    pub error: Option<String>,
}

/// Detailed status of a single workload.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WorkloadStatus {
    /// Provider-assigned identifier.
    pub id: String,
    /// Human-readable name.
    pub name: String,
    /// Image reference.
    pub image: String,
    /// Lifecycle state.
    pub state: WorkloadState,
    /// Whether the workload is currently running.
    pub running: bool,
    /// Whether provider health checks pass.
    pub healthy: bool,
    /// Whether all readiness checks pass (Kubernetes).
    pub ready: bool,
    /// When the workload started, if known.
    pub started_at: Option<DateTime<Utc>>,
    /// When the workload stopped, if known.
    pub stopped_at: Option<DateTime<Utc>>,
    /// Process exit code, if the workload has exited.
    pub exit_code: Option<i32>,
    /// Human-readable status message.
    pub message: String,
    /// Restart count (Kubernetes).
    pub restarts: u32,
}

/// Summary information returned by list operations.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkloadInfo {
    /// Provider-assigned identifier.
    pub id: String,
    /// Human-readable name.
    pub name: String,
    /// Image reference.
    pub image: String,
    /// Lifecycle state.
    pub state: WorkloadState,
    /// Labels attached to the workload.
    pub labels: HashMap<String, String>,
    /// Creation time.
    pub created: DateTime<Utc>,
    /// Kubernetes namespace.
    pub namespace: String,
}

/// Resource usage statistics.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct WorkloadStats {
    /// CPU usage as a percentage.
    pub cpu_percent: f64,
    /// Memory in use, in bytes.
    pub memory_usage: i64,
    /// Memory limit, in bytes.
    pub memory_limit: i64,
    /// Bytes received over the network.
    pub network_rx_bytes: i64,
    /// Bytes transmitted over the network.
    pub network_tx_bytes: i64,
    /// Bytes read from disk.
    pub disk_read_bytes: i64,
    /// Bytes written to disk.
    pub disk_write_bytes: i64,
    /// Number of processes/threads.
    pub pids: i64,
}

/// A workload lifecycle event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorkloadEvent {
    /// Workload identifier.
    pub id: String,
    /// Workload name.
    pub name: String,
    /// Event kind (e.g. `"start"`, `"stop"`, `"die"`, `"oom"`, `"restart"`).
    pub event: String,
    /// When the event occurred.
    pub timestamp: DateTime<Utc>,
    /// Human-readable detail.
    pub message: String,
}

/// Result of running a command inside a workload.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ExecResult {
    /// Command exit code.
    pub exit_code: i32,
    /// Captured standard output.
    pub stdout: String,
    /// Captured standard error.
    pub stderr: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_default_state_is_unknown() {
        let status = WorkloadStatus::default();
        assert_eq!(status.state, WorkloadState::Unknown);
        assert!(status.started_at.is_none());
        assert!(status.exit_code.is_none());
    }

    #[test]
    fn deploy_result_carries_initial_state() {
        let result = DeployResult {
            id: "abc123".into(),
            name: "api".into(),
            state: WorkloadState::Running,
        };
        assert!(result.state.is_running());
    }

    #[test]
    fn wait_result_default_is_success() {
        let result = WaitResult::default();
        assert_eq!(result.status_code, 0);
        assert!(result.error.is_none());
    }
}
