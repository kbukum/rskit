//! Bounded async worker pool with streaming events and cooperative cancellation.

#![warn(missing_docs)]

/// Provider/handler bridge adapters for interoperability with `rskit-provider`.
pub mod bridge;
/// Task dispatch strategies (e.g. round-robin).
pub mod dispatch;
/// Event types emitted by tasks during execution.
pub mod event;
/// [`Handler`] trait implemented by task executors.
pub mod handler;
/// [`Pool`] and [`PoolConfig`] for managing concurrent task execution.
pub mod pool;
/// Task specs and scheduling helpers built on the worker pool.
pub mod scheduler;
/// [`TaskHandle`] returned to callers after task submission.
pub mod task;

pub use bridge::{as_provider, from_provider};
pub use dispatch::{DispatchStrategy, RoundRobinDispatcher};
pub use event::{Event, EventKind, Progress};
pub use handler::Handler;
pub use pool::{OverflowPolicy, Pool, PoolConfig, PoolStats};
pub use scheduler::{
    ExecutionPlan, ResourceRequirements, Scheduler, SchedulerConfig, SchedulingDecision, TaskBatch,
    TaskSpec, WorkerScheduler,
};
pub use task::TaskHandle;

#[cfg(test)]
mod tests;
