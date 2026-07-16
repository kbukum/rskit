//! Deployment request and its nested specification types.

use std::collections::HashMap;
use std::time::Duration;

use crate::state::{RestartPolicy, WorkloadState};

/// Describes a workload to deploy.
///
/// Construct with [`DeployRequest::new`] and set the remaining fields directly.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DeployRequest {
    /// Human-readable identifier (container name, pod name).
    pub name: String,
    /// Image reference.
    pub image: String,
    /// Override entrypoint/command.
    pub command: Vec<String>,
    /// Arguments passed to the command.
    pub args: Vec<String>,
    /// Environment variables.
    pub environment: HashMap<String, String>,
    /// Key-value pairs for filtering and grouping.
    pub labels: HashMap<String, String>,
    /// Metadata annotations (Kubernetes).
    pub annotations: HashMap<String, String>,
    /// Working directory inside the workload.
    pub work_dir: String,
    /// CPU/memory constraints.
    pub resources: Option<ResourceConfig>,
    /// Network configuration.
    pub network: Option<NetworkConfig>,
    /// Mount points.
    pub volumes: Vec<VolumeMount>,
    /// Port mappings.
    pub ports: Vec<PortMapping>,
    /// Restart behavior after exit.
    pub restart_policy: RestartPolicy,
    /// Remove the workload after it exits (Docker).
    pub auto_remove: bool,
    /// Number of replicas (Kubernetes); `0` means the provider default.
    pub replicas: u32,
    /// Maximum run time; `None` means no limit.
    pub timeout: Option<Duration>,
    /// Target platform (e.g. `"linux/amd64"`).
    pub platform: String,
    /// Namespace (Kubernetes); empty means the default namespace.
    pub namespace: String,
    /// Service account (Kubernetes).
    pub service_account: String,
}

impl DeployRequest {
    /// Create a request for the given workload `name` and `image`.
    #[must_use]
    pub fn new(name: impl Into<String>, image: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            image: image.into(),
            ..Self::default()
        }
    }
}

/// Compute resource constraints.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ResourceConfig {
    /// CPU limit (e.g. `"0.5"`, `"1"`, `"500m"`).
    pub cpu_limit: String,
    /// Minimum CPU request (Kubernetes).
    pub cpu_request: String,
    /// Memory limit (e.g. `"512m"`, `"1g"`).
    pub memory_limit: String,
    /// Minimum memory request (Kubernetes).
    pub memory_request: String,
}

/// Workload networking.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct NetworkConfig {
    /// Network name, `"host"`, `"bridge"`, or `"none"`.
    pub mode: String,
    /// Custom DNS servers.
    pub dns: Vec<String>,
    /// Extra `/etc/hosts` entries.
    pub hosts: HashMap<String, String>,
}

/// Maps a workload port to a host port.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortMapping {
    /// Host port.
    pub host: u16,
    /// Workload container port.
    pub container: u16,
    /// Transport protocol (`"tcp"` when empty).
    pub protocol: String,
}

/// A storage mount.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct VolumeMount {
    /// Host path (Docker) or PVC/ConfigMap name (Kubernetes).
    pub source: String,
    /// Mount path inside the workload.
    pub target: String,
    /// Whether the mount is read-only.
    pub read_only: bool,
    /// Mount kind: `"bind"`, `"volume"`, `"configmap"`, `"secret"`, `"pvc"`
    /// (`"bind"` when empty).
    pub kind: String,
}

/// Controls log retrieval.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LogOptions {
    /// Return only the last `tail` lines; `0` means all.
    pub tail: usize,
    /// Return logs no older than this duration.
    pub since: Option<Duration>,
    /// Stream mode (for [`crate::LogStreamer`]).
    pub follow: bool,
}

/// Filters workloads in list operations.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ListFilter {
    /// Match all labels (AND).
    pub labels: HashMap<String, String>,
    /// Name prefix/pattern.
    pub name: String,
    /// Restrict to a single state.
    pub state: Option<WorkloadState>,
    /// Kubernetes namespace; empty means all.
    pub namespace: String,
}

/// Restricts which image events a [`crate::ImageEventWatcher`] returns.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImageEventFilter {
    /// Actions to match; empty means all actions.
    pub actions: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_sets_name_and_image_with_defaults() {
        let req = DeployRequest::new("api", "nginx:latest");
        assert_eq!(req.name, "api");
        assert_eq!(req.image, "nginx:latest");
        assert_eq!(req.restart_policy, RestartPolicy::No);
        assert!(req.timeout.is_none());
        assert!(req.command.is_empty());
    }

    #[test]
    fn deploy_request_is_cloneable_and_eq() {
        let mut req = DeployRequest::new("api", "nginx");
        req.labels.insert("team".into(), "core".into());
        req.timeout = Some(Duration::from_secs(30));
        assert_eq!(req.clone(), req);
    }

    #[test]
    fn nested_specs_default_empty() {
        assert_eq!(ResourceConfig::default(), ResourceConfig::default());
        assert!(NetworkConfig::default().dns.is_empty());
        assert!(ListFilter::default().state.is_none());
        assert_eq!(LogOptions::default().tail, 0);
        assert!(ImageEventFilter::default().actions.is_empty());
    }
}
