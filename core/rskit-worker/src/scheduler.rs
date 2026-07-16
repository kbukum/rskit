//! Task specs and scheduling helpers built on the worker pool.

use std::collections::BTreeMap;
use std::sync::Arc;

use futures::StreamExt;
use rskit_errors::AppResult;
use serde::{Deserialize, Serialize};

use crate::{Handler, Pool, PoolConfig};

/// Scheduler configuration.
///
/// Tasks are executed through [`Pool`], so submissions use the same bounded
/// queue and overflow semantics as [`PoolConfig`]. `max_concurrent` maps to the
/// pool's semaphore capacity and `queue_size` maps to the internal submit queue
/// capacity. Values below one are clamped to one when the pool config is built.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SchedulerConfig {
    /// Maximum concurrently executing tasks.
    #[serde(default = "default_max_concurrent")]
    pub max_concurrent: usize,
    /// Maximum queued task submissions.
    #[serde(default = "default_queue_size")]
    pub queue_size: usize,
}

impl Default for SchedulerConfig {
    fn default() -> Self {
        Self {
            max_concurrent: default_max_concurrent(),
            queue_size: default_queue_size(),
        }
    }
}

impl SchedulerConfig {
    /// Convert this scheduler config into the canonical worker pool config.
    ///
    /// The resulting pool uses a bounded queue. With the default overflow
    /// policy, `submit` waits for queue capacity instead of buffering
    /// indefinitely; alternate policies can be configured on the returned
    /// [`PoolConfig`] before constructing a pool.
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

/// Named task definition.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskSpec {
    /// Stable task name used in logs and scheduler decisions.
    pub name: String,
    /// Resource requirements declared by the task.
    #[serde(default)]
    pub resources: ResourceRequirements,
    /// Arbitrary scheduler labels.
    #[serde(default)]
    pub labels: BTreeMap<String, String>,
}

impl TaskSpec {
    /// Create a task spec with default resources.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            resources: ResourceRequirements::default(),
            labels: BTreeMap::new(),
        }
    }

    /// Set task resources.
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
    /// Logical CPU units requested by the task.
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
    /// Task selected for execution.
    pub task: String,
    /// Worker pool that should execute the task.
    pub pool: String,
    /// Human-readable reason for the placement.
    pub reason: String,
}

/// A batch of task specs that should be scheduled together.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskBatch {
    /// Stable batch name.
    pub name: String,
    /// Tasks in submission order.
    pub tasks: Vec<TaskSpec>,
}

impl TaskBatch {
    /// Create an empty task batch.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            tasks: Vec::new(),
        }
    }

    /// Add a task to this batch.
    #[must_use]
    pub fn with_task(mut self, task: TaskSpec) -> Self {
        self.tasks.push(task);
        self
    }

    /// Convert this batch into a stream of task specs.
    pub fn into_stream(self) -> impl futures::Stream<Item = TaskSpec> + Send + 'static {
        futures::stream::iter(self.tasks)
    }
}

/// Planned execution for a task batch.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ExecutionPlan {
    /// Batch being planned.
    pub batch: String,
    /// Scheduling decisions in task order.
    pub decisions: Vec<SchedulingDecision>,
}

/// Scheduler contract for task placement.
#[async_trait::async_trait]
pub trait Scheduler: Send + Sync {
    /// Decide where the task should execute.
    async fn schedule(&self, spec: &TaskSpec) -> AppResult<SchedulingDecision>;

    /// Build an execution plan for a batch using a task stream.
    async fn plan_batch(&self, batch: TaskBatch) -> AppResult<ExecutionPlan> {
        let name = batch.name.clone();
        let mut stream = batch.into_stream();
        let mut decisions = Vec::new();
        while let Some(task) = stream.next().await {
            decisions.push(self.schedule(&task).await?);
        }
        Ok(ExecutionPlan {
            batch: name,
            decisions,
        })
    }
}

/// Scheduler backed by a canonical `rskit-worker` pool configuration.
///
/// The scheduler is deterministic and does not create background work by
/// itself. Execution backpressure is enforced by the bounded [`Pool`] produced
/// by [`WorkerScheduler::pool`].
#[derive(Debug, Clone)]
pub struct WorkerScheduler {
    pool_name: String,
    config: SchedulerConfig,
}

impl WorkerScheduler {
    /// Create a scheduler targeting a named worker pool.
    #[must_use]
    pub fn new(pool_name: impl Into<String>, config: SchedulerConfig) -> Self {
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
impl Scheduler for WorkerScheduler {
    async fn schedule(&self, spec: &TaskSpec) -> AppResult<SchedulingDecision> {
        Ok(SchedulingDecision {
            task: spec.name.clone(),
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
        let scheduler = WorkerScheduler::new("default", SchedulerConfig::default());
        let decision = scheduler
            .schedule(&TaskSpec::new("images").with_label("tier", "batch"))
            .await
            .unwrap();

        assert_eq!(decision.task, "images");
        assert_eq!(decision.pool, "default");
        assert!(decision.reason.contains("capacity"));
    }

    #[tokio::test]
    async fn worker_scheduler_plans_batch_through_stream() {
        let scheduler = WorkerScheduler::new("default", SchedulerConfig::default());
        let plan = scheduler
            .plan_batch(
                TaskBatch::new("batch")
                    .with_task(TaskSpec::new("first"))
                    .with_task(TaskSpec::new("second")),
            )
            .await
            .unwrap();

        assert_eq!(plan.batch, "batch");
        assert_eq!(plan.decisions.len(), 2);
        assert_eq!(plan.decisions[0].task, "first");
        assert_eq!(plan.decisions[1].task, "second");
    }
}
