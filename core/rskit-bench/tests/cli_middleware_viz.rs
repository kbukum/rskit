use std::collections::HashMap;
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

use rskit_bench::cli::CliRunner;
use rskit_bench::curves::{CalibrationCurve, ConfusionMatrixDetail};
use rskit_bench::evaluator::{Evaluator, EvaluatorFunc};
use rskit_bench::middleware::{with_caching, with_timing};
use rskit_bench::result::{
    BenchRunResult, BenchRunSummary, BranchResult, DatasetInfo, MetricResult,
};
use rskit_bench::run_storage::{ListOptions, RunStorage};
use rskit_bench::types::Prediction;
use rskit_bench::viz::{render_calibration, render_comparison, render_confusion};
use rskit_errors::{AppError, AppResult, ErrorCode};

fn result(id: &str, timestamp: &str, tag: &str, f1: f64) -> BenchRunResult {
    BenchRunResult {
        id: id.to_owned(),
        schema: rskit_bench::schema::schema_url(),
        version: rskit_bench::schema::version(),
        timestamp: timestamp.to_owned(),
        tag: tag.to_owned(),
        duration_ms: 10,
        dataset: DatasetInfo {
            name: "dataset".to_owned(),
            version: "1".to_owned(),
            sample_count: 1,
            label_distribution: HashMap::new(),
        },
        metrics: vec![MetricResult {
            name: "classification".to_owned(),
            value: f1,
            values: HashMap::from([("f1".to_owned(), f1)]),
            detail: None,
        }],
        branches: HashMap::new(),
        samples: Vec::new(),
        curves: HashMap::new(),
        provenance: rskit_bench::RunProvenance::default(),
    }
}

#[derive(Default)]
struct MemoryRunStorage {
    runs: Vec<BenchRunResult>,
}

impl MemoryRunStorage {
    fn new(runs: Vec<BenchRunResult>) -> Self {
        Self { runs }
    }
}

impl RunStorage for MemoryRunStorage {
    fn save(&self, _result: &BenchRunResult) -> AppResult<String> {
        Err(AppError::new(ErrorCode::Internal, "save not supported"))
    }

    fn load(&self, run_id: &str) -> AppResult<BenchRunResult> {
        self.runs
            .iter()
            .find(|run| run.id == run_id)
            .cloned()
            .ok_or_else(|| AppError::new(ErrorCode::NotFound, format!("missing {run_id}")))
    }

    fn latest(&self) -> AppResult<BenchRunResult> {
        self.runs
            .iter()
            .max_by_key(|run| &run.timestamp)
            .cloned()
            .ok_or_else(|| AppError::new(ErrorCode::NotFound, "no runs"))
    }

    fn list(&self, opts: ListOptions) -> AppResult<Vec<BenchRunSummary>> {
        let mut runs = self
            .runs
            .iter()
            .filter(|run| opts.tag.as_ref().is_none_or(|tag| run.tag == *tag))
            .filter(|run| {
                opts.dataset
                    .as_ref()
                    .is_none_or(|dataset| run.dataset.name == *dataset)
            })
            .map(|run| BenchRunSummary {
                id: run.id.clone(),
                timestamp: run.timestamp.clone(),
                tag: run.tag.clone(),
                dataset: run.dataset.name.clone(),
                f1: run.metrics[0].values["f1"],
            })
            .collect::<Vec<_>>();
        runs.sort_by(|left, right| right.timestamp.cmp(&left.timestamp));
        if opts.limit > 0 {
            runs.truncate(opts.limit);
        }
        Ok(runs)
    }
}

#[tokio::test]
async fn middleware_caches_successes_and_times_only_successful_predictions() {
    let calls = Arc::new(AtomicUsize::new(0));
    let evaluator_calls = calls.clone();
    let evaluator = EvaluatorFunc::new("echo", move |input| {
        let evaluator_calls = evaluator_calls.clone();
        Box::pin(async move {
            evaluator_calls.fetch_add(1, Ordering::SeqCst);
            if input == b"fail" {
                return Err(AppError::new(ErrorCode::ExternalService, "boom"));
            }
            Ok(Prediction {
                sample_id: String::from_utf8_lossy(&input).to_string(),
                label: "ok".to_owned(),
                score: 0.9,
                ..Prediction::default()
            })
        })
    });

    let caching = with_caching(Box::new(evaluator));
    assert_eq!(caching.hit_count(), 0);
    assert_eq!(caching.miss_count(), 0);
    assert_eq!(caching.name(), "echo");
    assert!(caching.is_available().await);

    let first = caching.evaluate(b"same".to_vec()).await.unwrap();
    let second = caching.evaluate(b"same".to_vec()).await.unwrap();
    assert_eq!(first.sample_id, "same");
    assert_eq!(second.sample_id, "same");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(caching.hit_count(), 1);
    assert_eq!(caching.miss_count(), 1);
    assert!(caching.evaluate(b"fail".to_vec()).await.is_err());
    assert_eq!(caching.miss_count(), 2);

    let timed = with_timing(Box::new(caching));
    assert_eq!(timed.average(), std::time::Duration::ZERO);
    timed.evaluate(b"timed".to_vec()).await.unwrap();
    assert_eq!(timed.timings().len(), 1);
    assert_eq!(timed.timings()[0].0, "timed");
    assert!(timed.evaluate(b"fail".to_vec()).await.is_err());
    assert_eq!(timed.timings().len(), 1);
}

#[test]
fn cli_runner_renders_latest_specific_list_and_compare_errors() {
    let runner = CliRunner::with_storage(Box::new(MemoryRunStorage::new(vec![
        result("old", "2026-01-01T00:00:00Z", "main", 0.6),
        result("new", "2026-01-02T00:00:00Z", "main", 0.8),
    ])));

    let mut output = Vec::new();
    runner.show_latest(&mut output).unwrap();
    let rendered = String::from_utf8(output).unwrap();
    assert!(rendered.contains("# Bench Run: new"));

    let mut output = Vec::new();
    runner.show_run(&mut output, "old").unwrap();
    let rendered = String::from_utf8(output).unwrap();
    assert!(rendered.contains("# Bench Run: old"));

    let mut output = Vec::new();
    runner
        .list_runs(&mut output, ListOptions::default().with_tag("main"))
        .unwrap();
    let rendered = String::from_utf8(output).unwrap();
    assert!(rendered.contains("new"));
    assert!(rendered.contains("old"));
    assert!(rendered.contains("Total: 2 run(s)"));

    let mut output = Vec::new();
    runner.compare_runs(&mut output, "old", "new").unwrap();
    let rendered = String::from_utf8(output).unwrap();
    assert!(rendered.contains("Comparison: old"));
    assert!(rendered.contains("classification.f1"));

    let mut output = Vec::new();
    runner.compare_latest(&mut output).unwrap();
    assert!(
        String::from_utf8(output)
            .unwrap()
            .contains("Comparison: old")
    );

    let empty_runner = CliRunner::with_storage(Box::new(MemoryRunStorage::default()));
    let mut output = Vec::new();
    empty_runner
        .list_runs(&mut output, ListOptions::default())
        .unwrap();
    assert_eq!(String::from_utf8(output).unwrap(), "No runs found.\n");
    let error = empty_runner.compare_latest(&mut Vec::new()).unwrap_err();
    assert_eq!(error.code(), ErrorCode::InvalidInput);
}

#[test]
fn svg_renderers_handle_empty_inputs_clamping_sorting_and_xml_escape() {
    assert_eq!(
        render_confusion(
            &ConfusionMatrixDetail {
                labels: Vec::new(),
                matrix: Vec::new(),
                orientation: String::new(),
            },
            300,
            300,
        ),
        ""
    );
    assert_eq!(render_comparison(&HashMap::new(), 300, 300), "");

    let confusion = render_confusion(
        &ConfusionMatrixDetail {
            labels: vec!["yes<&".to_owned(), "no".to_owned()],
            matrix: vec![vec![10, 1], vec![2, 0]],
            orientation: "row=actual".to_owned(),
        },
        320,
        320,
    );
    assert!(confusion.contains("Confusion Matrix"));
    assert!(confusion.contains("yes&lt;&amp;"));
    assert!(confusion.contains(">10<"));
    assert!(!confusion.contains("\"\""));

    let calibration = render_calibration(
        &CalibrationCurve {
            predicted_probability: vec![-0.5, 0.5, 1.5],
            actual_frequency: vec![1.5, 0.5, -0.5],
            bin_count: vec![1, 2, 3],
        },
        320,
        240,
    );
    assert!(calibration.contains("Calibration Curve"));
    assert!(calibration.contains("Predicted Probability"));
    assert!(calibration.contains("Actual Frequency"));
    assert!(!calibration.contains("\"\""));

    let no_metric_comparison = render_comparison(
        &HashMap::from([(
            "branch-a".to_owned(),
            BranchResult {
                name: "branch-a".to_owned(),
                tier: 1,
                metrics: HashMap::new(),
                avg_score_positive: 0.0,
                avg_score_negative: 0.0,
                duration_ms: 1,
                errors: 0,
            },
        )]),
        320,
        240,
    );
    assert!(no_metric_comparison.contains("Branch Comparison"));
    assert!(!no_metric_comparison.contains("branch-a"));

    let comparison = render_comparison(
        &HashMap::from([
            (
                "branch-b".to_owned(),
                BranchResult {
                    name: "branch-b".to_owned(),
                    tier: 2,
                    metrics: HashMap::from([("f1".to_owned(), 0.0), ("accuracy".to_owned(), 0.0)]),
                    avg_score_positive: 0.0,
                    avg_score_negative: 0.0,
                    duration_ms: 2,
                    errors: 0,
                },
            ),
            (
                "branch-a".to_owned(),
                BranchResult {
                    name: "branch-a".to_owned(),
                    tier: 1,
                    metrics: HashMap::from([("f1".to_owned(), 0.8), ("accuracy".to_owned(), 0.9)]),
                    avg_score_positive: 0.0,
                    avg_score_negative: 0.0,
                    duration_ms: 1,
                    errors: 0,
                },
            ),
        ]),
        360,
        260,
    );
    assert!(comparison.contains("Branch Comparison"));
    assert!(comparison.find("branch-a").unwrap() < comparison.find("branch-b").unwrap());
    assert!(comparison.contains("accuracy"));
    assert!(comparison.contains("f1"));
    assert!(!comparison.contains("\"\""));
}
