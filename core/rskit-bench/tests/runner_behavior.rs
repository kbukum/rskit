use std::collections::HashMap;
use std::io::Write;
use std::sync::Arc;

use parking_lot::Mutex;
use rskit_bench::dataset_loader::DatasetLoader;
use rskit_bench::evaluator::EvaluatorFunc;
use rskit_bench::metric::{Suite, exact_match};
use rskit_bench::report_gen::Reporter;
use rskit_bench::result::{BenchRunResult, BenchRunSummary, DatasetInfo, MetricResult};
use rskit_bench::run_storage::{ListOptions, RunStorage};
use rskit_bench::types::{Prediction, string_label_mapper};
use rskit_bench::{BenchRunner, FixedClock, RunOptions};
use rskit_errors::{AppError, AppResult, ErrorCode};

fn fixture_dataset_dir() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/dataset")
}

fn exact_match_suite() -> Suite<String> {
    Suite::new(vec![exact_match()])
}

fn previous_result(value: f64) -> BenchRunResult {
    BenchRunResult {
        id: "previous".to_owned(),
        schema: rskit_bench::schema::schema_url(),
        version: rskit_bench::schema::version(),
        timestamp: "2026-01-01T00:00:00Z".to_owned(),
        tag: "candidate".to_owned(),
        duration_ms: 1,
        dataset: DatasetInfo {
            name: "candidate".to_owned(),
            version: String::new(),
            sample_count: 2,
            label_distribution: HashMap::from([("yes".to_owned(), 1), ("no".to_owned(), 1)]),
        },
        metrics: vec![MetricResult {
            name: "exact_match".to_owned(),
            value,
            values: HashMap::from([
                ("correct".to_owned(), value * 2.0),
                ("total".to_owned(), 2.0),
            ]),
            detail: None,
        }],
        branches: HashMap::new(),
        samples: Vec::new(),
        curves: HashMap::new(),
    }
}

#[derive(Clone, Default)]
struct RecordingStorage {
    saved: Arc<Mutex<Vec<BenchRunResult>>>,
    latest: Option<BenchRunResult>,
}

impl RecordingStorage {
    fn with_latest(latest: BenchRunResult) -> Self {
        Self {
            saved: Arc::new(Mutex::new(Vec::new())),
            latest: Some(latest),
        }
    }
}

impl RunStorage for RecordingStorage {
    fn save(&self, result: &BenchRunResult) -> AppResult<String> {
        self.saved.lock().push(result.clone());
        Ok(result.id.clone())
    }

    fn load(&self, run_id: &str) -> AppResult<BenchRunResult> {
        self.saved
            .lock()
            .iter()
            .find(|result| result.id == run_id)
            .cloned()
            .or_else(|| self.latest.clone().filter(|result| result.id == run_id))
            .ok_or_else(|| AppError::new(ErrorCode::NotFound, format!("missing run {run_id}")))
    }

    fn latest(&self) -> AppResult<BenchRunResult> {
        self.latest
            .clone()
            .or_else(|| self.saved.lock().last().cloned())
            .ok_or_else(|| AppError::new(ErrorCode::NotFound, "no runs"))
    }

    fn list(&self, opts: ListOptions) -> AppResult<Vec<BenchRunSummary>> {
        let mut summaries = self
            .saved
            .lock()
            .iter()
            .map(|result| BenchRunSummary {
                id: result.id.clone(),
                timestamp: result.timestamp.clone(),
                tag: result.tag.clone(),
                dataset: result.dataset.name.clone(),
                f1: result.metrics.first().map_or(0.0, |metric| metric.value),
            })
            .collect::<Vec<_>>();
        if opts.limit > 0 {
            summaries.truncate(opts.limit);
        }
        Ok(summaries)
    }
}

struct FailingReporter;

impl Reporter for FailingReporter {
    fn name(&self) -> &str {
        "failing"
    }

    fn generate(&self, _writer: &mut dyn Write, _result: &BenchRunResult) -> AppResult<()> {
        Err(AppError::new(ErrorCode::Internal, "render failed"))
    }
}

#[tokio::test]
async fn runner_records_successes_failures_metrics_storage_and_reporter_errors() {
    let loader = DatasetLoader::new(fixture_dataset_dir(), string_label_mapper())
        .with_manifest_file("custom-manifest.json");
    let storage = RecordingStorage::default();
    let saved = storage.saved.clone();
    let evaluator = EvaluatorFunc::new("fixture-evaluator", |input| {
        Box::pin(async move {
            let text = String::from_utf8_lossy(&input);
            if text.contains("no label") {
                return Err(AppError::new(
                    ErrorCode::ExternalService,
                    "model refused sample",
                ));
            }
            Ok(Prediction {
                sample_id: "predicted-by-content".to_owned(),
                label: "yes".to_owned(),
                score: 0.87,
                ..Prediction::default()
            })
        })
    });

    let result = BenchRunner::new()
        .register("fixture", Box::new(evaluator), 2)
        .with_metrics(exact_match_suite())
        .with_storage(Box::new(storage))
        .with_reporter(Box::new(FailingReporter))
        .with_clock(Arc::new(FixedClock::new(1_704_067_200, 42)))
        .run(
            &loader,
            RunOptions::default()
                .with_concurrency(0)
                .with_tag("release candidate"),
        )
        .await
        .unwrap();

    assert_eq!(result.id, "release-candidate-20240101-000000");
    assert_eq!(result.timestamp, "2024-01-01T00:00:00Z");
    assert_eq!(result.dataset.name, "release candidate");
    assert_eq!(result.dataset.sample_count, 2);
    assert_eq!(result.dataset.label_distribution["yes"], 1);
    assert_eq!(result.dataset.label_distribution["no"], 1);
    assert_eq!(result.branches["fixture"].tier, 2);
    assert_eq!(result.branches["fixture"].metrics["accuracy"], 0.5);
    assert_eq!(result.branches["fixture"].errors, 1);
    assert_eq!(result.metrics[0].name, "exact_match");
    assert_eq!(result.metrics[0].value, 1.0);
    assert_eq!(result.samples.len(), 2);
    assert!(result.samples.iter().any(|sample| sample.correct));
    assert!(
        result
            .samples
            .iter()
            .any(|sample| sample.error.contains("model refused sample"))
    );
    assert_eq!(saved.lock().len(), 1);
}

#[tokio::test]
async fn runner_can_fail_release_gate_when_latest_run_regresses() {
    let loader = DatasetLoader::new(fixture_dataset_dir(), string_label_mapper())
        .with_manifest_file("custom-manifest.json");
    let evaluator = EvaluatorFunc::new("regressing-evaluator", |_input| {
        Box::pin(async {
            Ok(Prediction {
                label: "wrong".to_owned(),
                score: 0.2,
                ..Prediction::default()
            })
        })
    });

    let error = BenchRunner::new()
        .register("regressing", Box::new(evaluator), 1)
        .with_metrics(exact_match_suite())
        .with_storage(Box::new(RecordingStorage::with_latest(previous_result(
            1.0,
        ))))
        .with_comparator(rskit_bench::compare::RunComparator::default())
        .with_clock(Arc::new(FixedClock::new(1_704_067_200, 10)))
        .run(
            &loader,
            RunOptions::default()
                .with_fail_on_regression(true)
                .with_target("exact_match", 0.95),
        )
        .await
        .unwrap_err();

    assert_eq!(error.code(), ErrorCode::Internal);
    assert!(error.message().contains("Regression detected"));
    assert!(error.message().contains("exact_match"));
}
