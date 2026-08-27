//! The [`BenchRunner`] — drives registered branches through the full run lifecycle.

use super::evaluation::{
    EvaluationHandler, EvaluationOutcome, SampleFailureContext, failed_sample_context,
};
use super::options::RunOptions;
use crate::compare::RunComparator;
use crate::dataset_loader::DatasetLoader;
use crate::eval_context::RNG_ALGORITHM;
use crate::evaluator::Evaluator;
use crate::execution::BenchExecutionPlan;
use crate::metric::Suite;
use crate::provenance::{ProvenanceProbe, RunProvenance, SystemProvenanceProbe, dataset_hash};
use crate::report_gen::Reporter;
use crate::result::{BenchRunResult, BenchSampleResult, BranchResult, DatasetInfo, MetricResult};
use crate::run_id::generate_run_id;
use crate::run_storage::RunStorage;
use crate::schema;
use crate::types::ScoredSample;
use futures::stream::{FuturesUnordered, StreamExt};
use rskit_errors::{AppError, AppResult, ErrorCode};
use rskit_util::time::{SharedClock, elapsed_millis, format_rfc3339, system_clock};
use rskit_worker::Pool;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::Instrument;

struct Branch<L> {
    name: String,
    evaluator: Arc<dyn Evaluator<L>>,
    tier: i32,
}

/// Main benchmark runner.
pub struct BenchRunner<L> {
    branches: Vec<Branch<L>>,
    metrics: Option<Suite<L>>,
    storage: Option<Box<dyn RunStorage>>,
    reporters: Vec<Box<dyn Reporter>>,
    comparator: Option<RunComparator>,
    clock: SharedClock,
    probe: Arc<dyn ProvenanceProbe>,
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
    /// Creates an empty benchmark runner.
    pub fn new() -> Self {
        Self {
            branches: Vec::new(),
            metrics: None,
            storage: None,
            reporters: Vec::new(),
            comparator: None,
            clock: system_clock(),
            probe: Arc::new(SystemProvenanceProbe),
        }
    }

    /// Registers an evaluator branch with a display name and tier.
    pub fn register(
        mut self,
        name: impl Into<String>,
        evaluator: Box<dyn Evaluator<L>>,
        tier: i32,
    ) -> Self {
        self.branches.push(Branch {
            name: name.into(),
            evaluator: Arc::from(evaluator),
            tier,
        });
        self
    }

    #[must_use]
    /// Sets the metric suite computed after evaluator branches finish.
    pub fn with_metrics(mut self, suite: Suite<L>) -> Self {
        self.metrics = Some(suite);
        self
    }

    #[must_use]
    /// Sets the result storage backend used for saving runs and loading comparison baselines.
    pub fn with_storage(mut self, storage: Box<dyn RunStorage>) -> Self {
        self.storage = Some(storage);
        self
    }

    #[must_use]
    /// Adds a reporter invoked after the benchmark result is assembled.
    pub fn with_reporter(mut self, reporter: Box<dyn Reporter>) -> Self {
        self.reporters.push(reporter);
        self
    }

    #[must_use]
    /// Sets the comparator used for optional regression checks against the latest stored run.
    pub fn with_comparator(mut self, comparator: RunComparator) -> Self {
        self.comparator = Some(comparator);
        self
    }

    /// Set the clock used for run IDs, timestamps, and elapsed durations.
    #[must_use]
    pub fn with_clock(mut self, clock: SharedClock) -> Self {
        self.clock = clock;
        self
    }

    /// Sets the provenance probe used to record host and source-control identity.
    ///
    /// Defaults to [`SystemProvenanceProbe`]. Inject a [`FixedProvenanceProbe`](crate::FixedProvenanceProbe) for deterministic, offline reproducibility tests.
    #[must_use]
    pub fn with_provenance_probe(mut self, probe: Arc<dyn ProvenanceProbe>) -> Self {
        self.probe = probe;
        self
    }

    /// Runs all registered evaluator branches over the loaded dataset and returns the benchmark result.
    pub async fn run(
        &self,
        loader: &DatasetLoader<L>,
        opts: RunOptions,
    ) -> AppResult<BenchRunResult> {
        let execution = BenchExecutionPlan::new(&opts.tag, opts.concurrency);
        let span = execution.operation.start_span("bench.run");
        span.in_scope(|| {
            tracing::info!(
                tag = %opts.tag,
                concurrency = opts.concurrency,
                "benchmark run started"
            );
        });
        let result = self
            .run_with_execution(loader, opts, &execution)
            .instrument(span)
            .await;
        match &result {
            Ok(_) => execution.operation.end_operation("ok", None),
            Err(error) => execution.operation.end_operation("error", Some(error)),
        }
        result
    }

    async fn run_with_execution(
        &self,
        loader: &DatasetLoader<L>,
        opts: RunOptions,
        execution: &BenchExecutionPlan,
    ) -> AppResult<BenchRunResult> {
        let start = self.clock.monotonic_millis();
        let loaded = loader.load()?;
        let samples = loaded.samples;
        let dataset_info = {
            let mut label_distribution: HashMap<String, usize> = HashMap::new();
            for s in &samples {
                *label_distribution.entry(s.label.to_string()).or_insert(0) += 1;
            }
            DatasetInfo {
                name: loaded.name.clone(),
                version: loaded.version.clone(),
                sample_count: samples.len(),
                label_distribution,
            }
        };

        let mut branch_results = HashMap::new();
        let mut all_scored: Vec<ScoredSample<L>> = Vec::new();
        let mut sample_results: Vec<BenchSampleResult> = Vec::new();

        for branch in &self.branches {
            let branch_start = self.clock.monotonic_millis();
            let handler = Arc::new(EvaluationHandler {
                evaluator: Arc::clone(&branch.evaluator),
                branch_name: branch.name.clone(),
                timeout_secs: opts.timeout_secs,
                clock: Arc::clone(&self.clock),
                seed: opts.seed,
            });
            let pool = Pool::new(handler, execution.pool_config_for(&branch.name));
            let mut branch_metrics: HashMap<String, f64> = HashMap::new();
            let mut total_score_pos = 0.0_f64;
            let mut total_score_neg = 0.0_f64;
            let mut count_pos = 0usize;
            let mut count_neg = 0usize;
            let mut errors = 0usize;
            let mut pending = FuturesUnordered::new();
            let mut sample_iter = samples.iter();
            let concurrency = opts.concurrency.max(1);
            loop {
                while pending.len() < concurrency {
                    let Some(sample) = sample_iter.next() else {
                        break;
                    };
                    let context = SampleFailureContext::from_sample(sample);
                    let handle = pool.submit(sample.clone()).await?;
                    let submitted_at = self.clock.monotonic_millis();
                    pending.push(async move { (context, submitted_at, handle.result().await) });
                }

                // FuturesUnordered retires whichever worker result completes first,
                // so a slow oldest task cannot block the submission window.
                let Some((submitted_sample, submitted_at, result)) = pending.next().await else {
                    break;
                };
                match result {
                    Ok(EvaluationOutcome::Success {
                        sample,
                        prediction: pred,
                        duration_ms,
                    }) => {
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
                            duration_ms,
                            error: String::new(),
                        });

                        all_scored.push(ScoredSample {
                            sample: sample.clone(),
                            prediction: pred,
                        });
                    }
                    Ok(EvaluationOutcome::Failure {
                        sample,
                        duration_ms,
                        error,
                    }) => {
                        tracing::warn!(
                            sample_id = %sample.id,
                            branch = %branch.name,
                            error = %error,
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
                            duration_ms,
                            error,
                        });
                    }
                    Err(error) => {
                        errors += 1;
                        tracing::warn!(
                            sample_id = %submitted_sample.id,
                            branch = %branch.name,
                            error = %error,
                            "worker evaluation failed"
                        );
                        sample_results.push(failed_sample_context(
                            &submitted_sample,
                            elapsed_millis(submitted_at, self.clock.monotonic_millis()),
                            format!("worker evaluation failed: {error}"),
                        ));
                    }
                }
            }
            pool.shutdown().await?;

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
                    duration_ms: elapsed_millis(branch_start, self.clock.monotonic_millis()),
                    errors,
                },
            );
        }

        let metric_results: Vec<MetricResult> = if let Some(ref suite) = self.metrics {
            suite.compute(&all_scored)?
        } else {
            Vec::new()
        };

        let duration_ms = elapsed_millis(start, self.clock.monotonic_millis());
        let epoch_seconds = self.clock.epoch_seconds();
        let run_id = generate_run_id(&opts.tag, epoch_seconds);
        let timestamp = i64::try_from(epoch_seconds)
            .ok()
            .and_then(format_rfc3339)
            .unwrap_or_else(|| "1970-01-01T00:00:00Z".to_string());

        let provenance = RunProvenance {
            seed: opts.seed,
            rng_algorithm: RNG_ALGORITHM.to_string(),
            git_commit: self.probe.git_commit(),
            tool_version: env!("CARGO_PKG_VERSION").to_string(),
            host: self.probe.host(),
            os: self.probe.os(),
            arch: self.probe.arch(),
            dataset_hash: dataset_hash(&samples),
            dataset_name: dataset_info.name.clone(),
            dataset_version: dataset_info.version.clone(),
            branches: self.branches.iter().map(|b| b.name.clone()).collect(),
            metrics: metric_results.iter().map(|m| m.name.clone()).collect(),
        };

        let result = BenchRunResult {
            id: run_id,
            schema: schema::schema_url(),
            version: schema::version(),
            timestamp,
            tag: opts.tag.clone(),
            duration_ms,
            dataset: dataset_info,
            metrics: metric_results,
            branches: branch_results,
            samples: sample_results,
            curves: HashMap::new(),
            provenance,
        };

        if let Some(ref storage) = self.storage {
            storage.save(&result)?;
        }

        let mut reporter_failures: Vec<(String, AppError)> = Vec::new();
        for reporter in &self.reporters {
            let mut buf = Vec::new();
            if let Err(e) = reporter.generate(&mut buf, &result) {
                tracing::error!(
                    reporter = reporter.name(),
                    error = %e,
                    "report generation failed"
                );
                reporter_failures.push((reporter.name().to_string(), e));
            }
        }
        if !reporter_failures.is_empty() {
            let failed_names: Vec<String> = reporter_failures
                .iter()
                .map(|(name, _)| name.clone())
                .collect();
            let (_, first_error) = reporter_failures.remove(0);
            return Err(first_error.context(format!(
                "report generation failed for {} reporter(s): {}",
                failed_names.len(),
                failed_names.join(", ")
            )));
        }

        if let (Some(comparator), Some(storage)) = (&self.comparator, &self.storage) {
            match storage.latest() {
                Ok(prev) => {
                    let diff = comparator.compare(&prev, &result);
                    if opts.fail_on_regression && diff.has_regression() {
                        return Err(AppError::new(
                            ErrorCode::Internal,
                            format!("Regression detected:\n{}", diff.summary()),
                        ));
                    }
                    tracing::info!("{}", diff.summary());
                }
                // No prior run is a legitimate first-run baseline, not a failure.
                Err(e) if e.code() == ErrorCode::NotFound => {
                    tracing::debug!("no previous run to compare against");
                }
                // A real failure to load the baseline must not silently disable the
                // regression gate: surface it when the caller demanded the gate.
                Err(e) => {
                    if opts.fail_on_regression {
                        return Err(e.context("cannot verify regression: loading baseline run"));
                    }
                    tracing::warn!(error = %e, "skipping regression comparison: baseline load failed");
                }
            }
        }

        Ok(result)
    }
}
