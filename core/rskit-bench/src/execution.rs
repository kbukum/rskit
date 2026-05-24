//! Benchmark execution planning with worker and observability integration.

use rskit_observability::OperationContext;
use rskit_worker::PoolConfig;

/// Execution plan shared by benchmark runners and CI smoke checks.
pub struct BenchExecutionPlan {
    run_name: String,
    concurrency: usize,
    /// Observability context for benchmark-level spans and metrics.
    pub operation: OperationContext,
}

impl BenchExecutionPlan {
    /// Build a plan for a benchmark run.
    #[must_use]
    pub fn new(name: impl Into<String>, concurrency: usize) -> Self {
        let name = name.into();
        Self {
            run_name: name.clone(),
            concurrency: concurrency.max(1),
            operation: OperationContext::new("rskit-bench", name, "benchmark", "system"),
        }
    }

    /// Build a fresh worker pool config for one benchmark branch.
    #[must_use]
    pub fn pool_config_for(&self, branch: &str) -> PoolConfig {
        PoolConfig::new(format!("bench-{}-{branch}", self.run_name)).with_size(self.concurrency)
    }
}

#[cfg(test)]
mod tests {
    use super::BenchExecutionPlan;

    #[test]
    fn plan_uses_worker_pool_and_observability_context() {
        let plan = BenchExecutionPlan::new("classification", 8);
        let pool = plan.pool_config_for("baseline");
        assert_eq!(pool.size, 8);
        assert_eq!(pool.name, "bench-classification-baseline");
        assert_eq!(plan.operation.service_name(), "rskit-bench");
    }
}
