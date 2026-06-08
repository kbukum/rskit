use std::collections::HashMap;
use std::path::PathBuf;

use rskit_bench::dataset::{load_content, load_manifest};
use rskit_bench::dataset_loader::DatasetLoader;
use rskit_bench::metrics::{ConfusionMatrix, ThresholdMetrics};
use rskit_bench::report::{RunResult, SampleResult, json_report, markdown_report};
use rskit_bench::report_gen::{
    CsvReporter, JUnitReporter, JsonReporter, MarkdownReporter, Reporter, TableReporter,
    VegaLiteReporter, vegalite_specs,
};
use rskit_bench::result::{
    BenchRunResult, BenchSampleResult, BranchResult, DatasetInfo, MetricResult,
};
use rskit_bench::types::string_label_mapper;
use rskit_errors::ErrorCode;
use serde_json::json;

fn fixture_dataset_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/dataset")
}

fn bench_result() -> BenchRunResult {
    let mut label_distribution = HashMap::new();
    label_distribution.insert("yes".to_owned(), 1);
    label_distribution.insert("no".to_owned(), 1);

    let mut values = HashMap::new();
    values.insert("precision".to_owned(), 0.8);
    values.insert("recall".to_owned(), 0.5);

    let mut branch_metrics = HashMap::new();
    branch_metrics.insert("f1".to_owned(), 0.75);

    let mut branches = HashMap::new();
    branches.insert(
        "very-long-branch-name-that-needs-truncation".to_owned(),
        BranchResult {
            name: "very-long-branch-name-that-needs-truncation".to_owned(),
            tier: 2,
            metrics: branch_metrics,
            avg_score_positive: 0.91,
            avg_score_negative: 0.12,
            duration_ms: 42,
            errors: 1,
        },
    );

    let mut branch_scores = HashMap::new();
    branch_scores.insert("branch-a".to_owned(), 0.4);

    let mut curves = HashMap::new();
    curves.insert(
        "roc".to_owned(),
        json!({"fpr": [0.0, 0.5, 1.0], "tpr": [0.0, 0.75, 1.0], "auc": 0.83}),
    );
    curves.insert(
        "score_distribution".to_owned(),
        json!([
            {"label": "yes", "bins": [0.1, 0.9], "counts": [1, 3]},
            {"label": "no", "bins": [0.2], "counts": [2]}
        ]),
    );
    curves.insert(
        "threshold_sweep".to_owned(),
        json!([
            {"threshold": 0.25, "precision": 1.0, "recall": 0.5, "f1": 0.667},
            {"threshold": 0.75, "precision": 0.5, "recall": 0.5, "f1": 0.5}
        ]),
    );
    curves.insert(
        "calibration".to_owned(),
        json!({"predicted_probability": [0.2, 0.8], "actual_frequency": [0.25, 0.75]}),
    );
    curves.insert(
        "ignored_invalid".to_owned(),
        json!({"not": "a known curve"}),
    );

    BenchRunResult {
        id: "run-1".to_owned(),
        schema: "https://schemas.skillsenselab.dev/rskit/bench/run-result/v1.json".to_owned(),
        version: "1.0.0".to_owned(),
        timestamp: "2026-06-07T00:00:00Z".to_owned(),
        tag: "release".to_owned(),
        duration_ms: 1_234,
        dataset: DatasetInfo {
            name: "fixture-dataset".to_owned(),
            version: "2.1.0".to_owned(),
            sample_count: 2,
            label_distribution,
        },
        metrics: vec![MetricResult {
            name: "accuracy".to_owned(),
            value: 0.5,
            values,
            detail: Some(json!({
                "labels": ["yes", "no"],
                "matrix": [[1, 0], [1, 0]]
            })),
        }],
        branches,
        samples: vec![
            BenchSampleResult {
                id: "positive".to_owned(),
                label: "yes".to_owned(),
                predicted: "yes".to_owned(),
                score: 0.91,
                correct: true,
                branch_scores: HashMap::new(),
                duration_ms: 10,
                error: String::new(),
            },
            BenchSampleResult {
                id: "negative".to_owned(),
                label: "no".to_owned(),
                predicted: "yes".to_owned(),
                score: 0.67,
                correct: false,
                branch_scores,
                duration_ms: 20,
                error: "ambiguous".to_owned(),
            },
        ],
        curves,
    }
}

#[test]
fn dataset_loader_honors_custom_manifest_filters_and_mapper_errors() {
    let dir = fixture_dataset_dir();
    let manifest = load_manifest(&dir);
    assert!(matches!(manifest, Err(error) if error.code() == ErrorCode::NotFound));

    let loader = DatasetLoader::new(&dir, string_label_mapper())
        .with_manifest_file("custom-manifest.json")
        .filter(|sample| sample.label == "yes");
    let samples = loader.all().unwrap();

    assert_eq!(samples.len(), 1);
    assert_eq!(samples[0].id, "loader-yes-sample");
    assert_eq!(samples[0].label, "yes");
    assert_eq!(samples[0].source, "fixture");
    assert_eq!(samples[0].metadata["tier"], 1);
    assert_eq!(
        samples[0].input,
        b"fixture payload used to verify custom manifest loading for a yes label\n"
    );

    let mapper_error = DatasetLoader::<String>::new(
        &dir,
        Box::new(|label: &str| {
            Err(rskit_errors::AppError::new(
                ErrorCode::InvalidInput,
                format!("unsupported label: {label}"),
            ))
        }),
    )
    .with_manifest_file("custom-manifest.json")
    .all();
    assert!(matches!(mapper_error, Err(error) if error.code() == ErrorCode::InvalidInput));
}

#[test]
fn dataset_manifest_and_content_report_missing_or_malformed_inputs() {
    let dir = fixture_dataset_dir();
    let sample = rskit_bench::dataset::Sample {
        id: "missing".to_owned(),
        file: "missing.txt".to_owned(),
        label: "yes".to_owned(),
        source: String::new(),
        description: String::new(),
        metadata: HashMap::new(),
    };

    assert!(matches!(
        load_content(&dir, &sample),
        Err(error) if error.code() == ErrorCode::Internal
    ));

    let temp = tempfile::tempdir().unwrap();
    std::fs::write(temp.path().join("manifest.json"), "{not-json").unwrap();
    assert!(matches!(
        load_manifest(temp.path()),
        Err(error) if error.code() == ErrorCode::Internal
    ));

    std::fs::write(
        temp.path().join("manifest.json"),
        r#"{"name":"bad","samples":[{"id":"s1","file":"missing.txt","label":"yes"}]}"#,
    )
    .unwrap();
    assert!(matches!(
        load_manifest(temp.path()),
        Err(error) if error.code() == ErrorCode::NotFound
    ));
}

#[test]
fn legacy_report_helpers_render_empty_branch_and_per_branch_cases() {
    let mut per_branch = HashMap::new();
    per_branch.insert(
        "branch-b".to_owned(),
        ThresholdMetrics {
            threshold: 0.5,
            precision: 0.5,
            recall: 1.0,
            f1: 0.667,
            accuracy: 0.5,
            fpr: 1.0,
            confusion: ConfusionMatrix {
                tp: 1,
                fp: 1,
                tn: 0,
                fn_count: 0,
            },
        },
    );

    let result = RunResult {
        run_id: "legacy-run".to_owned(),
        timestamp: "2026-06-07T00:00:00Z".to_owned(),
        tag: "candidate".to_owned(),
        dataset_name: "fixture".to_owned(),
        sample_results: vec![
            SampleResult {
                sample_id: "s1".to_owned(),
                label: "yes".to_owned(),
                is_positive: true,
                overall_score: 0.9,
                branch_scores: HashMap::from([("branch-b".to_owned(), 0.9)]),
                processing_ms: 12,
            },
            SampleResult {
                sample_id: "s2".to_owned(),
                label: "no".to_owned(),
                is_positive: false,
                overall_score: 0.6,
                branch_scores: HashMap::new(),
                processing_ms: 15,
            },
        ],
        metrics: ThresholdMetrics {
            threshold: 0.5,
            precision: 0.5,
            recall: 1.0,
            f1: 0.667,
            accuracy: 0.5,
            fpr: 1.0,
            confusion: ConfusionMatrix {
                tp: 1,
                fp: 1,
                tn: 0,
                fn_count: 0,
            },
        },
        per_branch,
    };

    let report = markdown_report(&result);
    assert!(report.contains("BENCH RUN: legacy-run"));
    assert!(report.contains("Tag: candidate"));
    assert!(report.contains("PER-BRANCH BREAKDOWN"));
    assert!(report.contains("branch-b"));

    let json = json_report(&result);
    assert_eq!(json["run_id"], "legacy-run");
    assert_eq!(json["per_branch"]["branch-b"]["f1"], 0.667);

    let without_branches = RunResult {
        tag: String::new(),
        per_branch: HashMap::new(),
        ..result
    };
    let report = markdown_report(&without_branches);
    assert!(!report.contains("PER-BRANCH BREAKDOWN"));
    assert!(!report.contains("Tag:"));
}

#[test]
fn report_generators_emit_all_supported_formats_and_escape_special_values() {
    let result = bench_result();
    let reporters: Vec<Box<dyn Reporter>> = vec![
        Box::new(JsonReporter),
        Box::new(CsvReporter),
        Box::new(MarkdownReporter),
        Box::new(TableReporter),
        Box::new(JUnitReporter::new("suite<&\"'>")),
        Box::new(VegaLiteReporter),
    ];

    for reporter in reporters {
        let mut output = Vec::new();
        reporter.generate(&mut output, &result).unwrap();
        let rendered = String::from_utf8(output).unwrap();
        assert!(
            !rendered.is_empty(),
            "{} reporter produced no output",
            reporter.name()
        );

        match reporter.name() {
            "json" => {
                let json: serde_json::Value = serde_json::from_str(&rendered).unwrap();
                assert_eq!(json["id"], "run-1");
                assert_eq!(json["metrics"][0]["values"]["precision"], 0.8);
            }
            "csv" => {
                assert!(rendered.contains("metric_name,value,details"));
                assert!(rendered.contains("accuracy.precision,0.800000,"));
                assert!(
                    rendered.contains(
                        "branch.very-long-branch-name-that-needs-truncation.f1,0.750000,"
                    )
                );
            }
            "markdown" => {
                assert!(rendered.contains("# Bench Run: run-1"));
                assert!(rendered.contains("## Confusion Matrix"));
                assert!(rendered.contains("### Incorrect Predictions"));
            }
            "table" => {
                assert!(rendered.contains("BENCH RUN: run-1"));
                assert!(rendered.contains("very-long-branch"));
                assert!(rendered.contains("Samples: 2 total, 1 correct (50.0%), 1 errors"));
            }
            "junit" => {
                assert!(rendered.contains("suite&lt;&amp;&quot;&apos;&gt;"));
                assert!(
                    rendered
                        .contains("<failure message=\"expected=no predicted=yes score=0.670\"/>")
                );
            }
            "vegalite" => {
                let json: serde_json::Value = serde_json::from_str(&rendered).unwrap();
                assert!(json.get("roc").is_some());
                assert!(json.get("confusion_matrix").is_some());
                assert!(json.get("score_distribution").is_some());
                assert!(json.get("threshold_sweep").is_some());
                assert!(json.get("calibration").is_some());
                assert!(json.get("branch_comparison").is_some());
            }
            name => panic!("unexpected reporter {name}"),
        }
    }
}

#[test]
fn vegalite_specs_skip_invalid_curve_shapes_without_failing() {
    let mut result = bench_result();
    result
        .curves
        .insert("roc".to_owned(), json!({"fpr": [0.0]}));
    result.curves.insert(
        "score_distribution".to_owned(),
        json!([{"label": "yes", "bins": [0.1]}]),
    );
    result
        .curves
        .insert("threshold_sweep".to_owned(), json!([{"precision": 1.0}]));
    result.curves.insert(
        "calibration".to_owned(),
        json!({"predicted_probability": [0.5]}),
    );
    result.metrics[0].detail = Some(json!({"labels": ["yes"]}));

    let specs = vegalite_specs(&result);
    assert!(specs.get("roc").is_none());
    assert!(specs.get("score_distribution").is_none());
    assert!(specs.get("threshold_sweep").is_none());
    assert!(specs.get("calibration").is_none());
    assert!(specs.get("confusion_matrix").is_none());
    assert!(specs.get("branch_comparison").is_some());
}
