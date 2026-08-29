//! Asynchronous metrics for I/O-backed scoring.
//!
//! [`AsyncMetric`] is the seam for metrics that must await external work — an embedding provider, an LLM judge — to score a run. It complements the synchronous [`Metric`] trait, which stays pure and deterministic for offline, CPU-only metrics. A resolved async metric can be surfaced back through the sync trait with [`as_sync`], so callers can either await metrics through [`Suite::compute_all`](super::Suite::compute_all) or precompute their results and feed them into the synchronous path.

use std::marker::PhantomData;

use async_trait::async_trait;
use rskit_errors::AppResult;

use super::Metric;
use crate::result::MetricResult;
use crate::types::ScoredSample;

/// Computes one benchmark metric that requires awaiting external work.
///
/// Unlike [`Metric`], an implementation may perform I/O (embedding or LLM provider calls). Every such call must carry its own timeout and cancellation so a slow or hung provider cannot stall a run. Failures return a typed error rather than a fabricated success-shaped result.
#[async_trait]
pub trait AsyncMetric<L = String>: Send + Sync {
    /// Returns the stable metric name used in benchmark results.
    fn name(&self) -> &str;

    /// Computes the metric result from scored samples, awaiting any external work.
    async fn compute(&self, scored: &[ScoredSample<L>]) -> AppResult<MetricResult>;
}

/// Bridges an already-resolved [`AsyncMetric`] result into the synchronous [`Metric`] trait.
///
/// The returned metric ignores the samples it is handed and always yields `precomputed`, so a caller that resolved async metrics during evaluation can surface them alongside sync metrics without re-running any I/O. The two paths are equivalent: awaiting [`AsyncMetric::compute`] and wrapping its result with `as_sync` produce the same [`MetricResult`].
pub fn as_sync<L>(precomputed: MetricResult) -> Box<dyn Metric<L>>
where
    L: Send + Sync + 'static,
{
    Box::new(Precomputed {
        name: precomputed.name.clone(),
        result: precomputed,
        _phantom: PhantomData,
    })
}

struct Precomputed<L> {
    name: String,
    result: MetricResult,
    _phantom: PhantomData<L>,
}

impl<L: Send + Sync + 'static> Metric<L> for Precomputed<L> {
    fn name(&self) -> &str {
        &self.name
    }

    fn compute(&self, _scored: &[ScoredSample<L>]) -> AppResult<MetricResult> {
        Ok(self.result.clone())
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::result::MetricDirection;

    struct CannedAsync {
        name: String,
        value: f64,
    }

    #[async_trait]
    impl AsyncMetric<String> for CannedAsync {
        fn name(&self) -> &str {
            &self.name
        }

        async fn compute(&self, _scored: &[ScoredSample<String>]) -> AppResult<MetricResult> {
            Ok(MetricResult {
                name: self.name.clone(),
                value: self.value,
                direction: MetricDirection::HigherIsBetter,
                values: HashMap::new(),
                detail: None,
            })
        }
    }

    #[tokio::test]
    async fn async_and_precompute_paths_are_equivalent() {
        let metric = CannedAsync {
            name: "canned".into(),
            value: 0.75,
        };
        let awaited = metric.compute(&[]).await.expect("async compute");

        let bridged = as_sync::<String>(awaited.clone());
        let sync_result = bridged.compute(&[]).expect("sync compute");

        assert_eq!(bridged.name(), "canned");
        assert_eq!(sync_result.name, awaited.name);
        assert_eq!(sync_result.value, awaited.value);
        assert_eq!(sync_result.direction, awaited.direction);
    }

    #[test]
    fn as_sync_ignores_samples_and_replays_result() {
        let precomputed = MetricResult {
            name: "semantic_similarity".into(),
            value: 0.9,
            direction: MetricDirection::HigherIsBetter,
            values: HashMap::from([("match_rate".into(), 1.0)]),
            detail: None,
        };
        let metric = as_sync::<String>(precomputed);
        // Two different (here empty) sample sets still replay the same result.
        let a = metric.compute(&[]).expect("compute a");
        let b = metric.compute(&[]).expect("compute b");
        assert_eq!(a.value, 0.9);
        assert_eq!(b.values["match_rate"], 1.0);
    }
}
