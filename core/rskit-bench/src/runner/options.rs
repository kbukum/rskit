//! Configuration for a benchmark run.

use std::collections::HashMap;

/// Options for configuring a benchmark run.
pub struct RunOptions {
    pub concurrency: usize,
    pub timeout_secs: u64,
    pub tag: String,
    pub fail_on_regression: bool,
    pub targets: HashMap<String, f64>,
}

impl Default for RunOptions {
    fn default() -> Self {
        Self {
            concurrency: 4,
            timeout_secs: 30,
            tag: String::from("default"),
            fail_on_regression: false,
            targets: HashMap::new(),
        }
    }
}

impl RunOptions {
    #[must_use]
    pub fn with_concurrency(mut self, n: usize) -> Self {
        self.concurrency = n;
        self
    }

    #[must_use]
    pub fn with_timeout(mut self, secs: u64) -> Self {
        self.timeout_secs = secs;
        self
    }

    #[must_use]
    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tag = tag.into();
        self
    }

    #[must_use]
    pub fn with_fail_on_regression(mut self, fail: bool) -> Self {
        self.fail_on_regression = fail;
        self
    }

    #[must_use]
    pub fn with_target(mut self, metric: impl Into<String>, threshold: f64) -> Self {
        self.targets.insert(metric.into(), threshold);
        self
    }
}
