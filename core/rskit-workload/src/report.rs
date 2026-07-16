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

/// Host-level information about a backend runtime.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SystemInfo {
    /// Operating system (e.g. `"linux"`, `"darwin"`).
    pub os: String,
    /// CPU architecture (e.g. `"x86_64"`, `"aarch64"`).
    pub architecture: String,
    /// Total host memory in mebibytes.
    pub total_memory_mb: i64,
    /// Number of logical CPUs.
    pub cpus: u32,
    /// Storage driver in use (e.g. `"overlay2"`).
    pub storage_driver: String,
    /// Runtime version (e.g. Docker/containerd version).
    pub runtime_version: String,
    /// Host kernel version.
    pub kernel_version: String,
    /// Full operating-system name (e.g. `"Ubuntu 24.04"`).
    pub operating_system: String,
    /// GPUs detected by the runtime.
    pub gpus: Vec<GpuInfo>,
}

/// A GPU detected by a backend runtime.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct GpuInfo {
    /// GPU model name.
    pub name: String,
    /// Total video memory in mebibytes.
    pub memory_mb: i64,
    /// GPU driver version.
    pub driver_version: String,
}

/// Storage consumption reported by a backend runtime.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DiskUsage {
    /// Total size of all images in bytes (may include shared layers).
    pub images_size: i64,
    /// Total size of all workloads/containers in bytes.
    pub containers_size: i64,
    /// Total size of all volumes in bytes.
    pub volumes_size: i64,
    /// Total build-cache size in bytes.
    pub build_cache_size: i64,
    /// Runtime data-root path (best effort; empty for remote daemons).
    pub data_root_path: String,
    /// Total filesystem capacity of the data root in bytes (`0` if unavailable).
    pub data_root_total: i64,
    /// Free filesystem space of the data root in bytes (`0` if unavailable).
    pub data_root_free: i64,
    /// Per-image disk usage breakdown.
    pub images: Vec<ImageDiskEntry>,
}

/// A single image in a [`DiskUsage`] breakdown.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageDiskEntry {
    /// Image identifier.
    pub id: String,
    /// Repository tags pointing at the image.
    pub repo_tags: Vec<String>,
    /// Unique size in bytes (excluding shared layers).
    pub size: i64,
    /// Size in bytes shared with other images.
    pub shared_size: i64,
    /// Image creation time.
    pub created: DateTime<Utc>,
}

/// Detailed metadata for a single image.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageDetail {
    /// Image identifier.
    pub id: String,
    /// Repository tags pointing at the image.
    pub repo_tags: Vec<String>,
    /// Repository digests for the image.
    pub repo_digests: Vec<String>,
    /// Image size in bytes.
    pub size: i64,
    /// Image CPU architecture.
    pub architecture: String,
    /// Image operating system.
    pub os: String,
    /// Image creation time.
    pub created: DateTime<Utc>,
    /// Selected container configuration baked into the image.
    pub config: ImageConfig,
    /// Layer digests, base to top.
    pub layers: Vec<String>,
}

/// Selected container configuration held in an image.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImageConfig {
    /// Default environment variables.
    pub env: Vec<String>,
    /// Image labels.
    pub labels: HashMap<String, String>,
    /// Default command.
    pub cmd: Vec<String>,
    /// Default entrypoint.
    pub entrypoint: Vec<String>,
}

/// An image lifecycle event reported by a backend runtime.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageEvent {
    /// Event action (e.g. `"pull"`, `"delete"`, `"tag"`, `"untag"`, `"import"`).
    pub action: String,
    /// Best-effort image reference.
    pub image_ref: String,
    /// Image identifier.
    pub image_id: String,
    /// When the event occurred.
    pub timestamp: DateTime<Utc>,
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

    #[test]
    fn system_info_default_is_empty() {
        let info = SystemInfo::default();
        assert_eq!(info.cpus, 0);
        assert!(info.gpus.is_empty());
    }

    #[test]
    fn disk_usage_carries_image_breakdown() {
        let usage = DiskUsage {
            images_size: 2048,
            images: vec![ImageDiskEntry {
                id: "img1".into(),
                repo_tags: vec!["nginx:latest".into()],
                size: 1024,
                shared_size: 512,
                created: Utc::now(),
            }],
            ..Default::default()
        };
        assert_eq!(usage.images.len(), 1);
        assert_eq!(usage.images[0].size, 1024);
    }

    #[test]
    fn image_detail_holds_config_and_layers() {
        let detail = ImageDetail {
            id: "img1".into(),
            repo_tags: vec!["nginx:latest".into()],
            repo_digests: Vec::new(),
            size: 4096,
            architecture: "amd64".into(),
            os: "linux".into(),
            created: Utc::now(),
            config: ImageConfig {
                env: vec!["PATH=/usr/bin".into()],
                ..Default::default()
            },
            layers: vec!["sha256:layer".into()],
        };
        assert_eq!(detail.config.env.len(), 1);
        assert_eq!(detail.layers.len(), 1);
    }

    #[test]
    fn image_event_records_action() {
        let event = ImageEvent {
            action: "pull".into(),
            image_ref: "nginx:latest".into(),
            image_id: "sha256:abc".into(),
            timestamp: Utc::now(),
        };
        assert_eq!(event.action, "pull");
    }

    #[test]
    fn gpu_info_default_is_empty() {
        assert_eq!(GpuInfo::default().memory_mb, 0);
    }
}
