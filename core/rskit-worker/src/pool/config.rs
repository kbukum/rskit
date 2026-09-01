//! Pool statistics, overflow policy, and configuration.

use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::dispatch::DispatchStrategy;

/// Statistics snapshot for the pool.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PoolStats {
    /// Human-readable name of the pool.
    pub name: String,
    /// Number of tasks currently executing.
    pub running: usize,
    /// Maximum concurrent tasks the pool allows.
    pub capacity: usize,
}

/// Overflow behavior applied when the submission queue is full.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
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
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
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
    #[serde(with = "rskit_util::time::serde_duration")]
    pub grace_period: Duration,
    /// Dispatch strategy (reserved for future multi-queue extensions).
    pub dispatch: DispatchStrategy,
    /// Queue overflow behavior.
    #[serde(rename = "overflow")]
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn overflow_policy_uses_snake_case_wire_strings() {
        assert_eq!(
            serde_json::to_value(OverflowPolicy::DropOldest).unwrap(),
            serde_json::json!("drop_oldest")
        );
        assert_eq!(
            serde_json::to_value(OverflowPolicy::Block).unwrap(),
            serde_json::json!("block")
        );
        let parsed: OverflowPolicy = serde_json::from_value(serde_json::json!("reject")).unwrap();
        assert_eq!(parsed, OverflowPolicy::Reject);
    }

    #[test]
    fn pool_config_loads_from_partial_document_with_overflow_key() {
        let config: PoolConfig = serde_json::from_value(serde_json::json!({
            "name": "ingest",
            "size": 8,
            "overflow": "drop_oldest",
            "grace_period": "5s"
        }))
        .unwrap();

        assert_eq!(config.name, "ingest");
        assert_eq!(config.size, 8);
        assert_eq!(config.overflow_policy, OverflowPolicy::DropOldest);
        assert_eq!(config.grace_period, Duration::from_secs(5));
        // Unspecified fields fall back to defaults.
        assert_eq!(config.queue_size, PoolConfig::default().queue_size);
    }

    #[test]
    fn pool_config_round_trips_through_json() {
        let config = PoolConfig::new("pool").with_overflow_policy(OverflowPolicy::Reject);
        let json = serde_json::to_string(&config).unwrap();
        let decoded: PoolConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(decoded.overflow_policy, OverflowPolicy::Reject);
        assert_eq!(decoded.grace_period, config.grace_period);
        assert!(json.contains("\"overflow\""));
    }

    #[test]
    fn pool_config_matches_cross_kit_golden_json() {
        let config = PoolConfig {
            name: "ingest".into(),
            size: 8,
            queue_size: 256,
            event_buffer: 64,
            grace_period: Duration::from_secs(3601),
            dispatch: DispatchStrategy::RoundRobin,
            overflow_policy: OverflowPolicy::Reject,
        };
        let actual = serde_json::to_string_pretty(&config).unwrap();
        let expected = include_str!("../../tests/fixtures/cross-kit/worker/pool-config.json");
        assert_eq!(format!("{actual}\n"), expected);

        let decoded: PoolConfig = serde_json::from_str(expected).unwrap();
        assert_eq!(decoded.name, config.name);
        assert_eq!(decoded.size, config.size);
        assert_eq!(decoded.queue_size, config.queue_size);
        assert_eq!(decoded.event_buffer, config.event_buffer);
        // The grace period round-trips losslessly even for a non-round value.
        assert_eq!(decoded.grace_period, Duration::from_secs(3601));
        assert_eq!(decoded.overflow_policy, OverflowPolicy::Reject);
    }
}
