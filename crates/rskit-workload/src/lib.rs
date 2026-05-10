#![warn(missing_docs)]
//! Workload management for rskit services.
//!
//! Provides abstractions for managing compute workloads, job scheduling,
//! and resource allocation. Mirrors `gokit/workload` and `pykit-workload`.

/// Workload manager configuration.
#[derive(Debug, Clone)]
pub struct WorkloadConfig {
    /// Maximum concurrent workloads.
    pub max_concurrent: usize,
}

impl Default for WorkloadConfig {
    fn default() -> Self {
        Self { max_concurrent: 4 }
    }
}
