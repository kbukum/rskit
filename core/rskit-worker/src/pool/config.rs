//! Pool statistics, overflow policy, and configuration.

use std::time::Duration;

use crate::dispatch::DispatchStrategy;

/// Statistics snapshot for the pool.
#[derive(Debug, Clone)]
pub struct PoolStats {
    /// Human-readable name of the pool.
    pub name: String,
    /// Number of tasks currently executing.
    pub running: usize,
    /// Maximum concurrent tasks the pool allows.
    pub capacity: usize,
}

/// Overflow behavior applied when the submission queue is full.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum OverflowPolicy {
    /// Wait until queue capacity becomes available.
    #[default]
    Block,
    /// Reject the new submission immediately.
    Reject,
    /// Drop the oldest queued task and enqueue the new submission.
    DropOldest,
}

/// Configuration for a [`Pool`](super::Pool).
pub struct PoolConfig {
    /// Human-readable name used in tracing.
    pub name: String,
    /// Maximum concurrent tasks (semaphore permits).
    pub size: usize,
    /// Capacity of the internal submit queue.
    pub queue_size: usize,
    /// Broadcast channel capacity for events per task.
    pub event_buffer: usize,
    /// Grace period given to in-flight tasks on shutdown.
    pub grace_period: Duration,
    /// Dispatch strategy (reserved for future multi-queue extensions).
    pub dispatch: DispatchStrategy,
    /// Queue overflow behavior.
    pub overflow_policy: OverflowPolicy,
}

impl Default for PoolConfig {
    fn default() -> Self {
        Self {
            name: "pool".into(),
            size: available_parallelism(),
            queue_size: 256,
            event_buffer: 64,
            grace_period: Duration::from_secs(30),
            dispatch: DispatchStrategy::RoundRobin,
            overflow_policy: OverflowPolicy::Block,
        }
    }
}

impl PoolConfig {
    /// Create a named pool configuration with sensible defaults.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            ..Default::default()
        }
    }

    /// Set the maximum number of concurrent tasks.
    /// Values below 1 are clamped to 1 inside `Pool::new` (with a tracing warning),
    /// since a zero-sized pool can never execute tasks.
    #[must_use]
    pub fn with_size(mut self, size: usize) -> Self {
        self.size = size;
        self
    }

    /// Set the capacity of the internal submit queue.
    #[must_use]
    pub fn with_queue_size(mut self, queue_size: usize) -> Self {
        self.queue_size = queue_size;
        self
    }

    /// Set the grace period given to in-flight tasks during shutdown.
    #[must_use]
    pub fn with_grace_period(mut self, d: Duration) -> Self {
        self.grace_period = d;
        self
    }

    /// Set the queue overflow policy.
    #[must_use]
    pub fn with_overflow_policy(mut self, overflow_policy: OverflowPolicy) -> Self {
        self.overflow_policy = overflow_policy;
        self
    }
}

fn available_parallelism() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
}
