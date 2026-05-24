#![warn(missing_docs)]
//! Workload management for rskit services.
//!
//! This crate defines serializable workload specs and thin scheduling helpers
//! that compose with `rskit-worker` instead of inventing an alternate execution
//! model.

use std::collections::BTreeMap;
use std::sync::Arc;

use futures::StreamExt;
use rskit_errors::AppResult;
use rskit_pipeline::from_slice;
use rskit_worker::{Handler, Pool, PoolConfig};
use serde::{Deserialize, Serialize};

/// Workload manager configuration.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkloadConfig {
    /// Maximum concurrently executing workload tasks.
    #[serde(default = "default_max_concurrent")]
    pub max_concurrent: usize,
    /// Maximum queued workload submissions.
    #[serde(default = "default_queue_size")]
    pub queue_size: usize,
}

impl Default for WorkloadConfig {
    fn default() -> Self {
        Self {
            max_concurrent: default_max_concurrent(),
            queue_size: default_queue_size(),
        }
    }
}

impl WorkloadConfig {
    /// Convert this workload config into the canonical worker pool config.
    #[must_use]
    pub fn to_pool_config(&self, name: impl Into<String>) -> PoolConfig {
        PoolConfig::new(name)
            .with_size(self.max_concurrent.max(1))
            .with_queue_size(self.queue_size.max(1))
    }
}

const fn default_max_concurrent() -> usize {
    4
}

const fn default_queue_size() -> usize {
    256
}

/// Named workload definition.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkloadSpec {
    /// Stable workload name used in logs and scheduler decisions.
    pub name: String,
    /// Resource requirements declared by the workload.
    #[serde(default)]
    pub resources: ResourceRequirements,
    /// Arbitrary scheduler labels.
    #[serde(default)]
    pub labels: BTreeMap<String, String>,
}

impl WorkloadSpec {
    /// Create a workload spec with default resources.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            resources: ResourceRequirements::default(),
            labels: BTreeMap::new(),
        }
    }

    /// Set workload resources.
    #[must_use]
    pub const fn with_resources(mut self, resources: ResourceRequirements) -> Self {
        self.resources = resources;
        self
    }

    /// Add a scheduler label.
    #[must_use]
    pub fn with_label(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.labels.insert(key.into(), value.into());
        self
    }
}

/// Resource requirements used by scheduling decisions.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResourceRequirements {
    /// Logical CPU units requested by the workload.
    #[serde(default = "default_cpu_units")]
    pub cpu_units: u32,
    /// Memory requested in mebibytes.
    #[serde(default = "default_memory_mib")]
    pub memory_mib: u64,
}

impl Default for ResourceRequirements {
    fn default() -> Self {
        Self {
            cpu_units: default_cpu_units(),
            memory_mib: default_memory_mib(),
        }
    }
}

const fn default_cpu_units() -> u32 {
    1
}

const fn default_memory_mib() -> u64 {
    128
}

/// Observable scheduler decision.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SchedulingDecision {
    /// Workload selected for execution.
    pub workload: String,
    /// Worker pool that should execute the workload.
    pub pool: String,
    /// Human-readable reason for the placement.
    pub reason: String,
}

/// A batch of workload specs that should be scheduled together.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkloadBatch {
    /// Stable batch name.
    pub name: String,
    /// Workloads in submission order.
    pub workloads: Vec<WorkloadSpec>,
}

impl WorkloadBatch {
    /// Create an empty workload batch.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            workloads: Vec::new(),
        }
    }

    /// Add a workload to this batch.
    #[must_use]
    pub fn with_workload(mut self, workload: WorkloadSpec) -> Self {
        self.workloads.push(workload);
        self
    }

    /// Convert this batch into the canonical rskit pipeline stream.
    pub fn into_stream(self) -> impl futures::Stream<Item = WorkloadSpec> + Send + 'static {
        from_slice(self.workloads)
    }
}

/// Planned execution for a workload batch.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExecutionPlan {
    /// Batch being planned.
    pub batch: String,
    /// Scheduling decisions in workload order.
    pub decisions: Vec<SchedulingDecision>,
}

/// Scheduler contract for workload placement.
#[async_trait::async_trait]
pub trait WorkloadScheduler: Send + Sync {
    /// Decide where the workload should execute.
    async fn schedule(&self, spec: &WorkloadSpec) -> AppResult<SchedulingDecision>;

    /// Build an execution plan for a batch using the canonical pipeline stream.
    async fn plan_batch(&self, batch: WorkloadBatch) -> AppResult<ExecutionPlan> {
        let name = batch.name.clone();
        let mut stream = batch.into_stream();
        let mut decisions = Vec::new();
        while let Some(workload) = stream.next().await {
            decisions.push(self.schedule(&workload).await?);
        }
        Ok(ExecutionPlan {
            batch: name,
            decisions,
        })
    }
}

/// Scheduler backed by a canonical `rskit-worker` pool configuration.
#[derive(Debug, Clone)]
pub struct WorkerScheduler {
    pool_name: String,
    config: WorkloadConfig,
}

impl WorkerScheduler {
    /// Create a scheduler targeting a named worker pool.
    #[must_use]
    pub fn new(pool_name: impl Into<String>, config: WorkloadConfig) -> Self {
        Self {
            pool_name: pool_name.into(),
            config,
        }
    }

    /// Create a typed worker pool for the supplied handler.
    #[must_use]
    pub fn pool<I, O, H>(&self, handler: Arc<H>) -> Pool<I, O>
    where
        I: Send + 'static,
        O: Clone + Send + 'static,
        H: Handler<I, O> + 'static,
    {
        Pool::new(handler, self.config.to_pool_config(self.pool_name.clone()))
    }
}

#[async_trait::async_trait]
impl WorkloadScheduler for WorkerScheduler {
    async fn schedule(&self, spec: &WorkloadSpec) -> AppResult<SchedulingDecision> {
        Ok(SchedulingDecision {
            workload: spec.name.clone(),
            pool: self.pool_name.clone(),
            reason: format!(
                "worker pool capacity={} queue={}",
                self.config.max_concurrent, self.config.queue_size
            ),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn worker_scheduler_returns_observable_decision() {
        let scheduler = WorkerScheduler::new("default", WorkloadConfig::default());
        let decision = scheduler
            .schedule(&WorkloadSpec::new("images").with_label("tier", "batch"))
            .await
            .unwrap();

        assert_eq!(decision.workload, "images");
        assert_eq!(decision.pool, "default");
        assert!(decision.reason.contains("capacity"));
    }

    #[tokio::test]
    async fn worker_scheduler_plans_batch_through_pipeline_stream() {
        let scheduler = WorkerScheduler::new("default", WorkloadConfig::default());
        let plan = scheduler
            .plan_batch(
                WorkloadBatch::new("batch")
                    .with_workload(WorkloadSpec::new("first"))
                    .with_workload(WorkloadSpec::new("second")),
            )
            .await
            .unwrap();

        assert_eq!(plan.batch, "batch");
        assert_eq!(plan.decisions.len(), 2);
        assert_eq!(plan.decisions[0].workload, "first");
        assert_eq!(plan.decisions[1].workload, "second");
    }
}
