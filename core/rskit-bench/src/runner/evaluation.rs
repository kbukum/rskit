//! Per-sample evaluation worker and its outcome.
//!
//! The [`BenchRunner`](super::BenchRunner) submits each sample to a worker
//! [`Pool`](rskit_worker::Pool) of [`EvaluationHandler`]s; each handler runs the
//! branch [`Evaluator`] under a timeout and reports an [`EvaluationOutcome`].

use crate::evaluator::Evaluator;
use crate::result::BenchSampleResult;
use crate::types::{BenchSample, Prediction};
use rskit_errors::AppResult;
use rskit_util::time::{SharedClock, elapsed_millis};
use rskit_worker::{Event, Handler};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;

/// A worker that evaluates a single sample against one branch's [`Evaluator`].
pub(super) struct EvaluationHandler<L> {
    pub(super) evaluator: Arc<dyn Evaluator<L>>,
    pub(super) branch_name: String,
    pub(super) timeout_secs: u64,
    pub(super) clock: SharedClock,
}

/// The identifying fields of a sample, retained so a worker failure can still be
/// reported after the sample itself has been moved into the pool.
pub(super) struct SampleFailureContext {
    pub(super) id: String,
    label: String,
}

impl SampleFailureContext {
    pub(super) fn from_sample<L: std::fmt::Display>(sample: &BenchSample<L>) -> Self {
        Self {
            id: sample.id.clone(),
            label: sample.label.to_string(),
        }
    }
}

/// The result of evaluating one sample: a scored prediction or a failure reason.
#[derive(Clone)]
pub(super) enum EvaluationOutcome<L> {
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

/// Build a failed [`BenchSampleResult`] from a retained sample context.
pub(super) fn failed_sample_context(
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
