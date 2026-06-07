//! Bench runner — orchestrates the complete benchmark lifecycle.

use crate::compare::RunComparator;
use crate::dataset_loader::DatasetLoader;
use crate::evaluator::Evaluator;
use crate::execution::BenchExecutionPlan;
use crate::metric::Suite;
use crate::report_gen::Reporter;
use crate::result::{BenchRunResult, BenchSampleResult, BranchResult, DatasetInfo, MetricResult};
use crate::run_id::generate_run_id;
use crate::run_storage::RunStorage;
use crate::schema;
use crate::types::{BenchSample, Prediction, ScoredSample};
use futures::stream::{FuturesUnordered, StreamExt};
use rskit_errors::{AppError, AppResult, ErrorCode};
use rskit_util::time::{SharedClock, elapsed_millis, format_rfc3339, system_clock};
use rskit_worker::{Event, Handler, Pool};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use tracing::Instrument;

struct Branch<L> {
    name: String,
    evaluator: Arc<dyn Evaluator<L>>,
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
    clock: SharedClock,
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
            clock: system_clock(),
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
            evaluator: Arc::from(evaluator),
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

    /// Set the clock used for run IDs, timestamps, and elapsed durations.
    #[must_use]
    pub fn with_clock(mut self, clock: SharedClock) -> Self {
        self.clock = clock;
        self
    }

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
            suite.compute(&all_scored)
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

        if let (Some(comparator), Some(storage)) = (&self.comparator, &self.storage)
            && let Ok(prev) = storage.latest()
        {
            let diff = comparator.compare(&prev, &result);
            if opts.fail_on_regression && diff.has_regression() {
                let error = AppError::new(
                    ErrorCode::Internal,
                    format!("Regression detected:\n{}", diff.summary()),
                );
                return Err(error);
            }
            tracing::info!("{}", diff.summary());
        }

        Ok(result)
    }
}

struct EvaluationHandler<L> {
    evaluator: Arc<dyn Evaluator<L>>,
    branch_name: String,
    timeout_secs: u64,
    clock: SharedClock,
}

struct SampleFailureContext {
    id: String,
    label: String,
}

impl SampleFailureContext {
    fn from_sample<L: std::fmt::Display>(sample: &BenchSample<L>) -> Self {
        Self {
            id: sample.id.clone(),
            label: sample.label.to_string(),
        }
    }
}

#[derive(Clone)]
enum EvaluationOutcome<L> {
    Success {
        sample: BenchSample<L>,
        prediction: Prediction<L>,
        duration_ms: u64,
    },
    Failure {
        sample: BenchSample<L>,
        duration_ms: u64,
        error: String,
    },
}

#[async_trait::async_trait]
impl<L> Handler<BenchSample<L>, EvaluationOutcome<L>> for EvaluationHandler<L>
where
    L: Clone + Send + Sync + std::fmt::Display + 'static,
{
    async fn handle(
        &self,
        sample: BenchSample<L>,
        _emit: mpsc::Sender<Event<EvaluationOutcome<L>>>,
        cancel: CancellationToken,
    ) -> AppResult<EvaluationOutcome<L>> {
        let start = self.clock.monotonic_millis();
        let input = sample.input.clone();
        let timeout = tokio::time::Duration::from_secs(self.timeout_secs);
        let eval = tokio::time::timeout(timeout, self.evaluator.evaluate(input));
        let result = tokio::select! {
            _ = cancel.cancelled() => {
                return Ok(EvaluationOutcome::Failure {
                    sample,
                    duration_ms: elapsed_millis(start, self.clock.monotonic_millis()),
                    error: "cancelled".to_string(),
                });
            }
            result = eval => result,
        };
        let duration_ms = elapsed_millis(start, self.clock.monotonic_millis());

        match result {
            Ok(Ok(prediction)) => Ok(EvaluationOutcome::Success {
                sample,
                prediction,
                duration_ms,
            }),
            Ok(Err(error)) => Ok(EvaluationOutcome::Failure {
                sample,
                duration_ms,
                error: error.to_string(),
            }),
            Err(_) => Ok(EvaluationOutcome::Failure {
                sample,
                duration_ms,
                error: format!("timeout in {}", self.branch_name),
            }),
        }
    }
}

fn failed_sample_context(
    sample: &SampleFailureContext,
    duration_ms: u64,
    error: String,
) -> BenchSampleResult {
    BenchSampleResult {
        id: sample.id.clone(),
        label: sample.label.clone(),
        predicted: String::new(),
        score: 0.0,
        correct: false,
        branch_scores: HashMap::new(),
        duration_ms,
        error,
    }
}
