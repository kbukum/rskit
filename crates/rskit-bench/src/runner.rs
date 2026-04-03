//! Bench runner — orchestrates the complete benchmark lifecycle.

use crate::compare::RunComparator;
use crate::dataset_loader::DatasetLoader;
use crate::evaluator::Evaluator;
use crate::metric::Suite;
use crate::report_gen::Reporter;
use crate::result::{BenchRunResult, BenchSampleResult, BranchResult, DatasetInfo, MetricResult};
use crate::run_storage::RunStorage;
use crate::schema;
use crate::storage::generate_run_id;
use crate::types::ScoredSample;
use rskit_errors::{AppError, AppResult, ErrorCode};
use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;
use tokio::sync::Semaphore;

struct Branch<L> {
    name: String,
    evaluator: Box<dyn Evaluator<L>>,
    tier: i32,
}

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
    pub fn with_concurrency(mut self, n: usize) -> Self {
        self.concurrency = n;
        self
    }

    pub fn with_timeout(mut self, secs: u64) -> Self {
        self.timeout_secs = secs;
        self
    }

    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tag = tag.into();
        self
    }

    pub fn with_fail_on_regression(mut self, fail: bool) -> Self {
        self.fail_on_regression = fail;
        self
    }

    pub fn with_target(mut self, metric: impl Into<String>, threshold: f64) -> Self {
        self.targets.insert(metric.into(), threshold);
        self
    }
}

/// Main benchmark runner.
pub struct BenchRunner<L> {
    branches: Vec<Branch<L>>,
    metrics: Option<Suite<L>>,
    storage: Option<Box<dyn RunStorage>>,
    reporters: Vec<Box<dyn Reporter>>,
    comparator: Option<RunComparator>,
}

impl<L> Default for BenchRunner<L>
where
    L: Clone + Send + Sync + std::fmt::Display + 'static,
{
    fn default() -> Self {
        Self::new()
    }
}

impl<L> BenchRunner<L>
where
    L: Clone + Send + Sync + std::fmt::Display + 'static,
{
    pub fn new() -> Self {
        Self {
            branches: Vec::new(),
            metrics: None,
            storage: None,
            reporters: Vec::new(),
            comparator: None,
        }
    }

    pub fn register(
        mut self,
        name: impl Into<String>,
        evaluator: Box<dyn Evaluator<L>>,
        tier: i32,
    ) -> Self {
        self.branches.push(Branch {
            name: name.into(),
            evaluator,
            tier,
        });
        self
    }

    pub fn with_metrics(mut self, suite: Suite<L>) -> Self {
        self.metrics = Some(suite);
        self
    }

    pub fn with_storage(mut self, storage: Box<dyn RunStorage>) -> Self {
        self.storage = Some(storage);
        self
    }

    pub fn with_reporter(mut self, reporter: Box<dyn Reporter>) -> Self {
        self.reporters.push(reporter);
        self
    }

    pub fn with_comparator(mut self, comparator: RunComparator) -> Self {
        self.comparator = Some(comparator);
        self
    }

    pub async fn run(
        &self,
        loader: &DatasetLoader<L>,
        opts: RunOptions,
    ) -> AppResult<BenchRunResult> {
        let start = Instant::now();

        let samples = loader.all()?;
        let dataset_info = {
            let mut label_distribution: HashMap<String, usize> = HashMap::new();
            for s in &samples {
                *label_distribution.entry(s.label.to_string()).or_insert(0) += 1;
            }
            DatasetInfo {
                name: opts.tag.clone(),
                version: String::new(),
                sample_count: samples.len(),
                label_distribution,
            }
        };

        let semaphore = Arc::new(Semaphore::new(opts.concurrency));
        let mut branch_results = HashMap::new();
        let mut all_scored: Vec<ScoredSample<L>> = Vec::new();
        let mut sample_results: Vec<BenchSampleResult> = Vec::new();

        for branch in &self.branches {
            let branch_start = Instant::now();
            let mut branch_metrics: HashMap<String, f64> = HashMap::new();
            let mut total_score_pos = 0.0_f64;
            let mut total_score_neg = 0.0_f64;
            let mut count_pos = 0usize;
            let mut count_neg = 0usize;
            let mut errors = 0usize;

            for sample in &samples {
                let _permit =
                    semaphore.clone().acquire_owned().await.map_err(|e| {
                        AppError::new(ErrorCode::Internal, format!("semaphore: {e}"))
                    })?;
                let input = sample.input.clone();
                let timeout = tokio::time::Duration::from_secs(opts.timeout_secs);

                let sample_start = Instant::now();
                let eval_result =
                    tokio::time::timeout(timeout, branch.evaluator.evaluate(input)).await;
                let sample_duration_ms = sample_start.elapsed().as_millis() as u64;

                match eval_result {
                    Ok(Ok(pred)) => {
                        let correct = pred.label.to_string() == sample.label.to_string();
                        if correct {
                            total_score_pos += pred.score;
                            count_pos += 1;
                        } else {
                            total_score_neg += pred.score;
                            count_neg += 1;
                        }

                        let mut branch_scores = HashMap::new();
                        branch_scores.insert(branch.name.clone(), pred.score);

                        sample_results.push(BenchSampleResult {
                            id: sample.id.clone(),
                            label: sample.label.to_string(),
                            predicted: pred.label.to_string(),
                            score: pred.score,
                            correct,
                            branch_scores,
                            duration_ms: sample_duration_ms,
                            error: String::new(),
                        });

                        all_scored.push(ScoredSample {
                            sample: sample.clone(),
                            prediction: pred,
                        });
                    }
                    Ok(Err(e)) => {
                        tracing::warn!(
                            sample_id = %sample.id,
                            branch = %branch.name,
                            error = %e,
                            "Evaluation failed"
                        );
                        errors += 1;
                        sample_results.push(BenchSampleResult {
                            id: sample.id.clone(),
                            label: sample.label.to_string(),
                            predicted: String::new(),
                            score: 0.0,
                            correct: false,
                            branch_scores: HashMap::new(),
                            duration_ms: sample_duration_ms,
                            error: e.to_string(),
                        });
                    }
                    Err(_) => {
                        tracing::warn!(
                            sample_id = %sample.id,
                            branch = %branch.name,
                            "Evaluation timed out"
                        );
                        errors += 1;
                        sample_results.push(BenchSampleResult {
                            id: sample.id.clone(),
                            label: sample.label.to_string(),
                            predicted: String::new(),
                            score: 0.0,
                            correct: false,
                            branch_scores: HashMap::new(),
                            duration_ms: sample_duration_ms,
                            error: "timeout".to_string(),
                        });
                    }
                }
            }

            let total = count_pos + count_neg + errors;
            if total > 0 {
                branch_metrics.insert("accuracy".to_string(), count_pos as f64 / total as f64);
            }

            let avg_score_positive = if count_pos > 0 {
                total_score_pos / count_pos as f64
            } else {
                0.0
            };
            let avg_score_negative = if count_neg > 0 {
                total_score_neg / count_neg as f64
            } else {
                0.0
            };

            branch_results.insert(
                branch.name.clone(),
                BranchResult {
                    name: branch.name.clone(),
                    tier: branch.tier,
                    metrics: branch_metrics,
                    avg_score_positive,
                    avg_score_negative,
                    duration_ms: branch_start.elapsed().as_millis() as u64,
                    errors,
                },
            );
        }

        let metric_results: Vec<MetricResult> = if let Some(ref suite) = self.metrics {
            suite.compute(&all_scored)
        } else {
            Vec::new()
        };

        let duration = start.elapsed();
        let run_id = generate_run_id(&opts.tag);
        let timestamp = {
            use std::time::SystemTime;
            let d = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default();
            format!("{}Z", d.as_secs())
        };

        let result = BenchRunResult {
            id: run_id,
            schema: schema::schema_url(),
            version: schema::version(),
            timestamp,
            tag: opts.tag.clone(),
            duration_ms: duration.as_millis() as u64,
            dataset: dataset_info,
            metrics: metric_results,
            branches: branch_results,
            samples: sample_results,
            curves: HashMap::new(),
        };

        if let Some(ref storage) = self.storage {
            storage.save(&result)?;
        }

        for reporter in &self.reporters {
            let mut buf = Vec::new();
            if let Err(e) = reporter.generate(&mut buf, &result) {
                tracing::warn!(
                    reporter = reporter.name(),
                    error = %e,
                    "Report generation failed"
                );
            }
        }

        if let (Some(comparator), Some(storage)) = (&self.comparator, &self.storage) {
            if let Ok(prev) = storage.latest() {
                let diff = comparator.compare(&prev, &result);
                if opts.fail_on_regression && diff.has_regression() {
                    return Err(AppError::new(
                        ErrorCode::Internal,
                        format!("Regression detected:\n{}", diff.summary()),
                    ));
                }
                tracing::info!("{}", diff.summary());
            }
        }

        Ok(result)
    }
}
