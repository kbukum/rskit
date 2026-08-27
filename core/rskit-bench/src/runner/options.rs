//! Configuration for a benchmark run.

use std::collections::HashMap;

use rand::SeedableRng;
use rand::rngs::StdRng;

/// Options for configuring a benchmark run.
pub struct RunOptions {
    /// Maximum number of in-flight sample evaluations per branch.
    pub concurrency: usize,
    /// Per-sample evaluation timeout in seconds.
    pub timeout_secs: u64,
    /// Run tag used in result metadata and generated run identifiers.
    pub tag: String,
    /// Whether to fail the run when comparison with the prior result detects a regression.
    pub fail_on_regression: bool,
    /// Metric target thresholds keyed by metric name.
    pub targets: HashMap<String, f64>,
    /// Deterministic seed recorded in run provenance and used to derive a
    /// reproducible RNG via [`RunOptions::seeded_rng`].
    pub seed: u64,
}

impl Default for RunOptions {
    fn default() -> Self {
        Self {
            concurrency: 4,
            timeout_secs: 30,
            tag: String::from("default"),
            fail_on_regression: false,
            targets: HashMap::new(),
            seed: 0,
        }
    }
}

impl RunOptions {
    #[must_use]
    /// Sets the maximum in-flight sample evaluations per branch.
    pub fn with_concurrency(mut self, n: usize) -> Self {
        self.concurrency = n;
        self
    }

    #[must_use]
    /// Sets the per-sample evaluation timeout in seconds.
    pub fn with_timeout(mut self, secs: u64) -> Self {
        self.timeout_secs = secs;
        self
    }

    #[must_use]
    /// Sets the run tag used in result metadata and generated run identifiers.
    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tag = tag.into();
        self
    }

    #[must_use]
    /// Enables or disables failing the run when regression comparison reports a significant regression.
    pub fn with_fail_on_regression(mut self, fail: bool) -> Self {
        self.fail_on_regression = fail;
        self
    }

    #[must_use]
    /// Adds or replaces a target threshold for a named metric.
    pub fn with_target(mut self, metric: impl Into<String>, threshold: f64) -> Self {
        self.targets.insert(metric.into(), threshold);
        self
    }

    #[must_use]
    /// Sets the deterministic run seed recorded in provenance and used by
    /// [`RunOptions::seeded_rng`].
    pub fn with_seed(mut self, seed: u64) -> Self {
        self.seed = seed;
        self
    }

    /// Returns a reproducible RNG derived from [`RunOptions::seed`].
    ///
    /// The same seed yields an identical sequence, so any sampling or shuffling an
    /// evaluator or metric performs can be made deterministic across runs.
    #[must_use]
    pub fn seeded_rng(&self) -> StdRng {
        StdRng::seed_from_u64(self.seed)
    }
}
