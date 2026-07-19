//! Provider-based workload orchestration.
//!
//! `rskit-workload` mirrors gokit's `workload` package:
//! a provider-agnostic [`Manager`] contract for deploying
//! and managing workloads (containers, pods, or any long-running unit),
//! an explicit backend [`WorkloadRegistry`],
//! and a lifecycle-managed [`WorkloadComponent`] that plugs into the shared component registry.
//!
//! The crate is foundational: it owns the *concept and vocabulary* of workload orchestration.
//! Concrete backends (Docker, Kubernetes, …) live in separate adapter crates
//! and register a [`ManagerFactory`] into a [`WorkloadRegistry`]; no backend is wired in implicitly.
//!
//! # Example
//!
//! ```rust
//! use rskit_workload::{WorkloadComponent, WorkloadConfig};
//! use rskit_component::Component;
//!
//! # #[tokio::main]
//! # async fn main() -> rskit_errors::AppResult<()> {
//! // Disabled config starts as a healthy no-op until a backend is registered.
//! let component = WorkloadComponent::new(WorkloadConfig::default());
//! component.start().await?;
//! assert!(component.health().is_healthy());
//! component.stop().await?;
//! # Ok(())
//! # }
//! ```

#![warn(missing_docs)]

/// Lifecycle-managed workload component.
pub mod component;
/// Provider-agnostic workload configuration.
pub mod config;
/// Workload lifecycle manager and optional capability traits.
pub mod manager;
/// Explicit workload backend registry.
pub mod registry;
/// Runtime state and result reports.
pub mod report;
/// CPU and memory quantity parsing and formatting.
pub mod resources;
/// Deployment request and its nested specification types.
pub mod spec;
/// Workload runtime state and restart policy.
pub mod state;

#[cfg(test)]
mod test_support;

pub use component::WorkloadComponent;
pub use config::WorkloadConfig;
pub use manager::{
    DiskUsageCapable, EventWatcher, ExecCapable, ImageEventWatcher, ImageInspector, LogStreamer,
    Manager, StatsCapable, SystemInfoCapable,
};
pub use registry::{ManagerFactory, WorkloadRegistry};
pub use report::{
    DeployResult, DiskUsage, ExecResult, GpuInfo, ImageConfig, ImageDetail, ImageDiskEntry,
    ImageEvent, SystemInfo, WaitResult, WorkloadEvent, WorkloadInfo, WorkloadStats, WorkloadStatus,
};
pub use resources::{format_cpu, format_memory, parse_cpu, parse_memory};
pub use spec::{
    DeployRequest, ImageEventFilter, ListFilter, LogOptions, NetworkConfig, PortMapping,
    ResourceConfig, VolumeMount,
};
pub use state::{RestartPolicy, WorkloadState};
