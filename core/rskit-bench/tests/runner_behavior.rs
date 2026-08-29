use std::collections::HashMap;
use std::io::Write;
use std::sync::Arc;

use parking_lot::Mutex;
use rskit_bench::dataset_loader::DatasetLoader;
use rskit_bench::evaluator::EvaluatorFunc;
use rskit_bench::metric::{Suite, exact_match};
use rskit_bench::report_gen::Reporter;
use rskit_bench::result::{
    BenchRunResult, BenchRunSummary, DatasetInfo, MetricDirection, MetricResult,
};
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
    let mut r = BenchRunResult::default();
    r.id = "previous".to_owned();
    r.schema = rskit_bench::schema::schema_url();
    r.version = rskit_bench::schema::version();
    r.timestamp = "2026-01-01T00:00:00Z".to_owned();
    r.tag = "candidate".to_owned();
    r.duration_ms = 1;
    r.dataset = DatasetInfo {
        name: "candidate".to_owned(),
        version: String::new(),
        sample_count: 2,
        label_distribution: HashMap::from([("yes".to_owned(), 1), ("no".to_owned(), 1)]),
    };
    r.metrics = vec![MetricResult {
        name: "exact_match".to_owned(),
        value,
        values: HashMap::from([
            ("correct".to_owned(), value * 2.0),
            ("total".to_owned(), 2.0),
        ]),
        direction: MetricDirection::HigherIsBetter,
        detail: None,
    }];
    r
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

#[derive(Clone, Default)]
struct RecordingReporter {
    calls: Arc<Mutex<usize>>,
}

impl Reporter for RecordingReporter {
    fn name(&self) -> &str {
        "recording"
    }

    fn generate(&self, writer: &mut dyn Write, result: &BenchRunResult) -> AppResult<()> {
        *self.calls.lock() += 1;
        writer
            .write_all(result.id.as_bytes())
            .map_err(|e| AppError::new(ErrorCode::Internal, format!("write: {e}")))
    }
}

/// Storage whose `latest()` fails with a non-`NotFound` error, modelling a real
/// baseline-load failure (corrupt file, permission denied) rather than a missing
/// first-run baseline.
struct BrokenLatestStorage;

impl RunStorage for BrokenLatestStorage {
    fn save(&self, result: &BenchRunResult) -> AppResult<String> {
        Ok(result.id.clone())
    }

    fn load(&self, run_id: &str) -> AppResult<BenchRunResult> {
        Err(AppError::new(
            ErrorCode::NotFound,
            format!("missing {run_id}"),
        ))
    }

    fn latest(&self) -> AppResult<BenchRunResult> {
        Err(AppError::new(
            ErrorCode::ExternalService,
            "baseline store unreachable",
        ))
    }

    fn list(&self, _opts: ListOptions) -> AppResult<Vec<BenchRunSummary>> {
        Ok(Vec::new())
    }
}

#[tokio::test]
async fn runner_records_successes_failures_metrics_and_storage() {
    let loader = DatasetLoader::new(fixture_dataset_dir(), string_label_mapper())
        .with_manifest_file("custom-manifest.json");
    let storage = RecordingStorage::default();
    let saved = storage.saved.clone();
    let reporter = RecordingReporter::default();
    let reporter_calls = reporter.calls.clone();
    let evaluator = EvaluatorFunc::new("fixture-evaluator", |input, _ctx| {
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
        .with_reporter(Box::new(reporter))
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
    assert_eq!(result.dataset.name, "fixture-dataset");
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
    assert_eq!(*reporter_calls.lock(), 1);
}

#[tokio::test]
async fn runner_surfaces_reporter_failure_instead_of_swallowing_it() {
    let loader = DatasetLoader::new(fixture_dataset_dir(), string_label_mapper())
        .with_manifest_file("custom-manifest.json");
    let evaluator = EvaluatorFunc::new("fixture-evaluator", |_input, _ctx| {
        Box::pin(async {
            Ok(Prediction {
                label: "yes".to_owned(),
                score: 0.9,
                ..Prediction::default()
            })
        })
    });

    let error = BenchRunner::new()
        .register("fixture", Box::new(evaluator), 1)
        .with_metrics(exact_match_suite())
        .with_reporter(Box::new(FailingReporter))
        .with_clock(Arc::new(FixedClock::new(1_704_067_200, 42)))
        .run(&loader, RunOptions::default().with_concurrency(1))
        .await
        .expect_err("a failing reporter must not be reported as a successful run");

    assert_eq!(error.code(), ErrorCode::Internal);
    assert!(
        error.message().contains("failing"),
        "message: {}",
        error.message()
    );
}

#[tokio::test]
async fn runner_fails_gate_when_baseline_load_errors_and_regression_gate_is_on() {
    let loader = DatasetLoader::new(fixture_dataset_dir(), string_label_mapper())
        .with_manifest_file("custom-manifest.json");
    let evaluator = EvaluatorFunc::new("fixture-evaluator", |_input, _ctx| {
        Box::pin(async {
            Ok(Prediction {
                label: "yes".to_owned(),
                score: 0.9,
                ..Prediction::default()
            })
        })
    });

    let error = BenchRunner::new()
        .register("fixture", Box::new(evaluator), 1)
        .with_metrics(exact_match_suite())
        .with_storage(Box::new(BrokenLatestStorage))
        .with_comparator(rskit_bench::compare::RunComparator::default())
        .with_clock(Arc::new(FixedClock::new(1_704_067_200, 10)))
        .run(&loader, RunOptions::default().with_fail_on_regression(true))
        .await
        .expect_err("a baseline-load failure must not silently pass the regression gate");

    assert_eq!(error.code(), ErrorCode::ExternalService);
    assert!(
        error.message().contains("cannot verify regression"),
        "message: {}",
        error.message()
    );
}

#[tokio::test]
async fn runner_first_run_passes_gate_without_a_baseline() {
    let loader = DatasetLoader::new(fixture_dataset_dir(), string_label_mapper())
        .with_manifest_file("custom-manifest.json");
    let evaluator = EvaluatorFunc::new("fixture-evaluator", |_input, _ctx| {
        Box::pin(async {
            Ok(Prediction {
                label: "yes".to_owned(),
                score: 0.9,
                ..Prediction::default()
            })
        })
    });

    // Empty storage: `latest()` returns NotFound, which is a legitimate first run.
    let result = BenchRunner::new()
        .register("fixture", Box::new(evaluator), 1)
        .with_metrics(exact_match_suite())
        .with_storage(Box::new(RecordingStorage::default()))
        .with_comparator(rskit_bench::compare::RunComparator::default())
        .with_clock(Arc::new(FixedClock::new(1_704_067_200, 10)))
        .run(&loader, RunOptions::default().with_fail_on_regression(true))
        .await
        .expect("first run without a baseline must pass the gate");

    assert_eq!(result.metrics[0].name, "exact_match");
}

#[tokio::test]
async fn runner_can_fail_release_gate_when_latest_run_regresses() {
    let loader = DatasetLoader::new(fixture_dataset_dir(), string_label_mapper())
        .with_manifest_file("custom-manifest.json");
    let evaluator = EvaluatorFunc::new("regressing-evaluator", |_input, _ctx| {
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

#[tokio::test]
async fn runner_records_reproducible_provenance() {
    async fn run_once() -> BenchRunResult {
        let loader = DatasetLoader::new(fixture_dataset_dir(), string_label_mapper())
            .with_manifest_file("custom-manifest.json");
        let evaluator = EvaluatorFunc::new("fixture-evaluator", |_input, _ctx| {
            Box::pin(async move {
                Ok(Prediction {
                    sample_id: "p".to_owned(),
                    label: "yes".to_owned(),
                    score: 0.9,
                    ..Prediction::default()
                })
            })
        });
        let probe = Arc::new(
            rskit_bench::FixedProvenanceProbe::new()
                .with_git_commit("abc123")
                .with_host("ci-runner")
                .with_os("linux")
                .with_arch("x86_64"),
        );
        BenchRunner::new()
            .register("fixture", Box::new(evaluator), 1)
            .with_metrics(exact_match_suite())
            .with_clock(Arc::new(FixedClock::new(1_704_067_200, 42)))
            .with_provenance_probe(probe)
            .run(&loader, RunOptions::default().with_seed(7).with_tag("eval"))
            .await
            .unwrap()
    }

    let first = run_once().await;
    let second = run_once().await;

    let provenance = &first.provenance;
    assert_eq!(provenance.seed, 7);
    assert_eq!(provenance.rng_algorithm, rskit_bench::RNG_ALGORITHM);
    assert_eq!(provenance.git_commit.as_deref(), Some("abc123"));
    assert_eq!(provenance.host, "ci-runner");
    assert_eq!(provenance.os, "linux");
    assert_eq!(provenance.arch, "x86_64");
    assert_eq!(provenance.dataset_name, "fixture-dataset");
    assert_eq!(provenance.dataset_version, "2.1.0");
    assert!(!provenance.dataset_hash.is_empty());
    assert_eq!(provenance.branches, vec!["fixture".to_owned()]);
    assert_eq!(provenance.metrics, vec!["exact_match".to_owned()]);
    assert!(!provenance.tool_version.is_empty());

    // Reproducibility: identical inputs (fixed clock + probe + seed) yield an
    // identical full result, not just identical provenance. Compared structurally
    // so unordered maps (e.g. label distribution) don't cause spurious failures.
    assert_eq!(
        serde_json::to_value(&first).unwrap(),
        serde_json::to_value(&second).unwrap()
    );
}
