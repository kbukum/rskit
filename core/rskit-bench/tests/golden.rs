//! Golden / snapshot tests for rskit-bench metric computations.
//!
//! Every test constructs deterministic, hand-crafted data with known expected
//! outcomes and uses `insta` to lock in exact numerical outputs.

use std::collections::HashMap;
use std::io::Cursor;

use rskit_bench::compare::RunComparator;
use rskit_bench::metric::{
    Suite,
    // probability
    auc_roc,
    // classification
    binary_classification,
    brier_score,
    calibration,
    confusion_matrix,
    // matching
    exact_match,
    fuzzy_match,
    log_loss,
    // regression
    mae,
    // ranking
    mean_average_precision,
    mse,
    multi_class_classification,
    ndcg,
    precision_at_k,
    r_squared,
    recall_at_k,
    rmse,
    threshold_sweep,
};
use rskit_bench::report_gen::{JsonReporter, MarkdownReporter, Reporter};
use rskit_bench::result::{
    BenchRunResult, BenchSampleResult, DatasetInfo, MetricDirection, MetricResult,
};
use rskit_bench::types::{BenchSample, Prediction, ScoredSample};
use rskit_errors::ErrorCode;

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Build a binary classification scored-sample.
fn binary_sample(id: &str, true_label: &str, pred_label: &str, score: f64) -> ScoredSample<String> {
    ScoredSample {
        sample: BenchSample {
            id: id.into(),
            input: vec![],
            label: true_label.into(),
            source: String::new(),
            metadata: HashMap::new(),
        },
        prediction: Prediction {
            sample_id: id.into(),
            label: pred_label.into(),
            score,
            scores: HashMap::new(),
            metadata: HashMap::new(),
        },
    }
}

/// Build a regression scored-sample (label type = f64).
fn regression_sample(id: &str, actual: f64, predicted: f64) -> ScoredSample<f64> {
    ScoredSample {
        sample: BenchSample {
            id: id.into(),
            input: vec![],
            label: actual,
            source: String::new(),
            metadata: HashMap::new(),
        },
        prediction: Prediction {
            sample_id: id.into(),
            label: predicted,
            score: predicted,
            scores: HashMap::new(),
            metadata: HashMap::new(),
        },
    }
}

/// Build a minimal BenchRunResult for report / comparison tests.
fn make_run_result(
    id: &str,
    tag: &str,
    metrics: Vec<MetricResult>,
    samples: Vec<BenchSampleResult>,
) -> BenchRunResult {
    let mut r = BenchRunResult::default();
    r.id = id.into();
    r.schema = "https://gokit.dev/bench/v1/schema.json".into();
    r.version = "1.0".into();
    r.timestamp = "2025-01-15T12:00:00Z".into();
    r.tag = tag.into();
    r.duration_ms = 42;
    r.dataset = DatasetInfo {
        name: "test-dataset".into(),
        version: "1.0".into(),
        sample_count: samples.len(),
        label_distribution: HashMap::new(),
    };
    r.metrics = metrics;
    r.samples = samples;
    r
}

/// Round an f64 to `d` decimal places (for stable snapshots).
fn round(v: f64, d: i32) -> f64 {
    let factor = 10f64.powi(d);
    (v * factor).round() / factor
}

/// Convert a MetricResult into a deterministic JSON value with sorted keys
/// and floats rounded to 6 decimal places.
fn stable_metric(m: &MetricResult) -> serde_json::Value {
    let mut sorted_values: Vec<(String, f64)> = m
        .values
        .iter()
        .map(|(k, v)| (k.clone(), round(*v, 6)))
        .collect();
    sorted_values.sort_by(|a, b| a.0.cmp(&b.0));
    let values_map: serde_json::Map<String, serde_json::Value> = sorted_values
        .into_iter()
        .map(|(k, v)| (k, serde_json::json!(v)))
        .collect();

    serde_json::json!({
        "name": m.name,
        "value": round(m.value, 6),
        "values": values_map,
    })
}

// ---------------------------------------------------------------------------
// 1. Classification metrics
// ---------------------------------------------------------------------------

/// 20 samples: 10 positive ("pos"), 10 negative ("neg").
/// Scores spread so that threshold=0.5 gives:
///   TP=7, FP=2, FN=3, TN=8  → accuracy=0.75, precision≈0.778, recall=0.7, F1≈0.737
fn classification_data() -> Vec<ScoredSample<String>> {
    // Positives (label="pos")
    let positives = vec![
        ("p01", 0.95), // TP (score >= 0.5)
        ("p02", 0.88), // TP
        ("p03", 0.76), // TP
        ("p04", 0.72), // TP
        ("p05", 0.65), // TP
        ("p06", 0.58), // TP
        ("p07", 0.52), // TP
        ("p08", 0.40), // FN (score < 0.5)
        ("p09", 0.30), // FN
        ("p10", 0.15), // FN
    ];
    // Negatives (label="neg")
    let negatives = vec![
        ("n01", 0.10), // TN
        ("n02", 0.15), // TN
        ("n03", 0.20), // TN
        ("n04", 0.25), // TN
        ("n05", 0.30), // TN
        ("n06", 0.35), // TN
        ("n07", 0.42), // TN
        ("n08", 0.48), // TN
        ("n09", 0.55), // FP (score >= 0.5)
        ("n10", 0.62), // FP
    ];

    let mut data = Vec::new();
    for (id, score) in positives {
        // For binary classification metric, label matching doesn't matter; only score & threshold.
        // But we set pred_label to match label when score >= 0.5.
        let pred = if score >= 0.5 { "pos" } else { "neg" };
        data.push(binary_sample(id, "pos", pred, score));
    }
    for (id, score) in negatives {
        let pred = if score >= 0.5 { "pos" } else { "neg" };
        data.push(binary_sample(id, "neg", pred, score));
    }
    data
}

#[test]
fn golden_binary_classification() {
    let data = classification_data();
    let m = binary_classification("pos".to_string(), 0.5);
    let result = m.compute(&data).unwrap();
    insta::assert_json_snapshot!("binary_classification", stable_metric(&result));
}

#[test]
fn golden_confusion_matrix() {
    let data = classification_data();
    let m = confusion_matrix(vec!["pos".to_string(), "neg".to_string()]);
    let result = m.compute(&data).unwrap();
    // detail has the matrix; snapshot the whole result
    insta::assert_json_snapshot!("confusion_matrix", stable_metric(&result));
}

#[test]
fn golden_threshold_sweep() {
    let data = classification_data();
    let m = threshold_sweep("pos".to_string(), None);
    let result = m.compute(&data).unwrap();
    // Snapshot value (best F1) and the detail array of ThresholdPoints
    let detail_val: serde_json::Value = result.detail.clone().unwrap_or_default();
    insta::assert_json_snapshot!(
        "threshold_sweep",
        serde_json::json!({
            "best_f1": round(result.value, 6),
            "points": detail_val,
        })
    );
}

#[test]
fn golden_multi_class_classification() {
    // 3-class: "cat", "dog", "bird" — 12 samples total
    let samples = vec![
        // Correct predictions
        binary_sample("m01", "cat", "cat", 0.9),
        binary_sample("m02", "cat", "cat", 0.8),
        binary_sample("m03", "dog", "dog", 0.85),
        binary_sample("m04", "dog", "dog", 0.7),
        binary_sample("m05", "bird", "bird", 0.95),
        binary_sample("m06", "bird", "bird", 0.6),
        // Incorrect predictions
        binary_sample("m07", "cat", "dog", 0.4), // cat misclassified as dog
        binary_sample("m08", "cat", "bird", 0.3), // cat misclassified as bird
        binary_sample("m09", "dog", "cat", 0.45), // dog misclassified as cat
        binary_sample("m10", "dog", "bird", 0.35), // dog misclassified as bird
        binary_sample("m11", "bird", "cat", 0.5), // bird misclassified as cat
        binary_sample("m12", "bird", "dog", 0.25), // bird misclassified as dog
    ];
    let m = multi_class_classification(vec![
        "cat".to_string(),
        "dog".to_string(),
        "bird".to_string(),
    ]);
    let result = m.compute(&samples).unwrap();
    insta::assert_json_snapshot!("multi_class_classification", stable_metric(&result));
}

// ---------------------------------------------------------------------------
// 2. Regression metrics
// ---------------------------------------------------------------------------

fn regression_data() -> Vec<ScoredSample<f64>> {
    // 10 samples with known actual/predicted pairs
    vec![
        regression_sample("r01", 3.0, 2.8),
        regression_sample("r02", 5.0, 5.2),
        regression_sample("r03", 1.5, 1.4),
        regression_sample("r04", 4.0, 4.5),
        regression_sample("r05", 2.0, 2.1),
        regression_sample("r06", 6.0, 5.5),
        regression_sample("r07", 3.5, 3.8),
        regression_sample("r08", 7.0, 6.8),
        regression_sample("r09", 2.5, 2.9),
        regression_sample("r10", 4.5, 4.3),
    ]
}

#[test]
fn golden_mae() {
    let data = regression_data();
    let m = mae();
    let result = m.compute(&data).unwrap();
    insta::assert_json_snapshot!("mae", stable_metric(&result));
}

#[test]
fn golden_mse() {
    let data = regression_data();
    let m = mse();
    let result = m.compute(&data).unwrap();
    insta::assert_json_snapshot!("mse", stable_metric(&result));
}

#[test]
fn golden_rmse() {
    let data = regression_data();
    let m = rmse();
    let result = m.compute(&data).unwrap();
    insta::assert_json_snapshot!("rmse", stable_metric(&result));
}

#[test]
fn golden_r_squared() {
    let data = regression_data();
    let m = r_squared();
    let result = m.compute(&data).unwrap();
    insta::assert_json_snapshot!("r_squared", stable_metric(&result));
}

// ---------------------------------------------------------------------------
// 3. Ranking metrics
// ---------------------------------------------------------------------------

fn ranking_data() -> Vec<ScoredSample<String>> {
    // 10 items. "relevant" = label matches prediction label.
    // Scores determine ranking order.
    vec![
        binary_sample("rk01", "pos", "pos", 0.95), // relevant, rank 1
        binary_sample("rk02", "neg", "pos", 0.90), // not relevant (label != pred only matters for ndcg)
        binary_sample("rk03", "pos", "pos", 0.85), // relevant, rank 3
        binary_sample("rk04", "pos", "pos", 0.80), // relevant, rank 4
        binary_sample("rk05", "neg", "neg", 0.75), // relevant (label==pred)
        binary_sample("rk06", "neg", "pos", 0.70), // not relevant
        binary_sample("rk07", "pos", "neg", 0.65), // not relevant (label!=pred)
        binary_sample("rk08", "pos", "pos", 0.60), // relevant
        binary_sample("rk09", "neg", "neg", 0.55), // relevant
        binary_sample("rk10", "neg", "pos", 0.50), // not relevant
    ]
}

#[test]
fn golden_ndcg_at_5() {
    let data = ranking_data();
    let m = ndcg::<String>(5);
    let result = m.compute(&data).unwrap();
    insta::assert_json_snapshot!("ndcg_at_5", stable_metric(&result));
}

#[test]
fn golden_ndcg_at_10() {
    let data = ranking_data();
    let m = ndcg::<String>(10);
    let result = m.compute(&data).unwrap();
    insta::assert_json_snapshot!("ndcg_at_10", stable_metric(&result));
}

#[test]
fn golden_mean_average_precision() {
    let data = ranking_data();
    let m = mean_average_precision("pos".to_string());
    let result = m.compute(&data).unwrap();
    insta::assert_json_snapshot!("mean_average_precision", stable_metric(&result));
}

#[test]
fn golden_precision_at_k() {
    let data = ranking_data();
    let m = precision_at_k("pos".to_string(), 5);
    let result = m.compute(&data).unwrap();
    insta::assert_json_snapshot!("precision_at_k_5", stable_metric(&result));
}

#[test]
fn golden_recall_at_k() {
    let data = ranking_data();
    let m = recall_at_k("pos".to_string(), 5);
    let result = m.compute(&data).unwrap();
    insta::assert_json_snapshot!("recall_at_k_5", stable_metric(&result));
}

// ---------------------------------------------------------------------------
// 4. Probability metrics
// ---------------------------------------------------------------------------

fn probability_data() -> Vec<ScoredSample<String>> {
    // 20 samples with calibrated-ish scores for "pos" label.
    vec![
        // True positives with high scores
        binary_sample("pr01", "pos", "pos", 0.95),
        binary_sample("pr02", "pos", "pos", 0.90),
        binary_sample("pr03", "pos", "pos", 0.85),
        binary_sample("pr04", "pos", "pos", 0.80),
        binary_sample("pr05", "pos", "pos", 0.75),
        binary_sample("pr06", "pos", "pos", 0.70),
        binary_sample("pr07", "pos", "pos", 0.65),
        // True positives with lower scores
        binary_sample("pr08", "pos", "pos", 0.55),
        binary_sample("pr09", "pos", "neg", 0.40),
        binary_sample("pr10", "pos", "neg", 0.30),
        // True negatives with low scores
        binary_sample("pr11", "neg", "neg", 0.10),
        binary_sample("pr12", "neg", "neg", 0.15),
        binary_sample("pr13", "neg", "neg", 0.20),
        binary_sample("pr14", "neg", "neg", 0.25),
        binary_sample("pr15", "neg", "neg", 0.30),
        binary_sample("pr16", "neg", "neg", 0.35),
        binary_sample("pr17", "neg", "neg", 0.40),
        // False positives (neg label with high score)
        binary_sample("pr18", "neg", "pos", 0.60),
        binary_sample("pr19", "neg", "pos", 0.70),
        binary_sample("pr20", "neg", "pos", 0.80),
    ]
}

#[test]
fn golden_auc_roc() {
    let data = probability_data();
    let m = auc_roc("pos".to_string());
    let result = m.compute(&data).unwrap();
    // Snapshot the scalar AUC value
    insta::assert_json_snapshot!(
        "auc_roc",
        serde_json::json!({
            "name": result.name,
            "value": round(result.value, 6),
        })
    );
}

#[test]
fn golden_brier_score() {
    let data = probability_data();
    let m = brier_score("pos".to_string());
    let result = m.compute(&data).unwrap();
    insta::assert_json_snapshot!("brier_score", stable_metric(&result));
}

#[test]
fn golden_log_loss() {
    let data = probability_data();
    let m = log_loss("pos".to_string());
    let result = m.compute(&data).unwrap();
    insta::assert_json_snapshot!("log_loss", stable_metric(&result));
}

#[test]
fn golden_calibration() {
    let data = probability_data();
    let m = calibration("pos".to_string(), 5);
    let result = m.compute(&data).unwrap();
    // ECE value + calibration curve detail
    let detail_val = result.detail.clone().unwrap_or_default();
    insta::assert_json_snapshot!(
        "calibration",
        serde_json::json!({
            "name": result.name,
            "ece": round(result.value, 6),
            "curve": detail_val,
        })
    );
}

// ---------------------------------------------------------------------------
// 5. Matching metrics
// ---------------------------------------------------------------------------

#[test]
fn golden_exact_match() {
    let data = vec![
        binary_sample("em01", "hello", "hello", 1.0),
        binary_sample("em02", "world", "world", 1.0),
        binary_sample("em03", "foo", "bar", 0.5),
        binary_sample("em04", "test", "test", 1.0),
        binary_sample("em05", "abc", "xyz", 0.2),
    ];
    let m = exact_match::<String>();
    let result = m.compute(&data).unwrap();
    insta::assert_json_snapshot!("exact_match", stable_metric(&result));
}

#[test]
fn golden_fuzzy_match() {
    let data = vec![
        binary_sample("fm01", "hello", "helo", 1.0), // similarity ≈ 0.8
        binary_sample("fm02", "world", "world", 1.0), // similarity = 1.0
        binary_sample("fm03", "kitten", "sitting", 0.5), // different
        binary_sample("fm04", "test", "tset", 0.8),  // similarity ≈ 0.5
        binary_sample("fm05", "abc", "abc", 1.0),    // exact
    ];
    let m = fuzzy_match::<String>(0.7);
    let result = m.compute(&data).unwrap();
    assert!(
        !result.values.contains_key("threshold"),
        "threshold is a configuration input, not a quality signal, so it must not appear in values"
    );
    let detail = result.detail.clone().expect("provenance detail present");
    assert_eq!(detail["threshold"], 0.7);
    insta::assert_json_snapshot!("fuzzy_match", stable_metric(&result));
}

// ---------------------------------------------------------------------------
// 6. Classification threshold compatibility
// ---------------------------------------------------------------------------

#[test]
fn classification_metric_matches_legacy_threshold_formula() {
    let scores = [0.9, 0.8, 0.7, 0.6, 0.55, 0.45, 0.35, 0.25, 0.15, 0.05];
    let labels = [
        true, true, true, true, false, true, false, false, false, false,
    ];
    let data: Vec<_> = scores
        .iter()
        .zip(labels.iter())
        .enumerate()
        .map(|(idx, (score, positive))| {
            let label = if *positive { "pos" } else { "neg" };
            let pred = if *score >= 0.5 { "pos" } else { "neg" };
            binary_sample(&format!("threshold-{idx}"), label, pred, *score)
        })
        .collect();

    let metric = binary_classification("pos".to_owned(), 0.5);
    let result = metric.compute(&data).unwrap();

    assert_eq!(result.values["tp"], 4.0);
    assert_eq!(result.values["fp"], 1.0);
    assert_eq!(result.values["tn"], 4.0);
    assert_eq!(result.values["fn"], 1.0);
    assert_eq!(round(result.values["precision"], 4), 0.8);
    assert_eq!(round(result.values["recall"], 4), 0.8);
    assert_eq!(round(result.values["f1"], 4), 0.8);
    assert_eq!(round(result.values["accuracy"], 4), 0.8);
    assert_eq!(round(result.values["fpr"], 4), 0.2);
    assert!(
        !result.values.contains_key("threshold"),
        "threshold is a configuration input, not a quality signal, so it must not appear in values"
    );
    let detail = result.detail.expect("provenance detail present");
    assert_eq!(detail["threshold"], 0.5);
}

#[test]
fn classification_identity_folds_threshold_for_comparison_safety() {
    let data = classification_data();
    let low = binary_classification("pos".to_string(), 0.3)
        .compute(&data)
        .unwrap();
    let high = binary_classification("pos".to_string(), 0.7)
        .compute(&data)
        .unwrap();

    assert_eq!(low.name, "classification[t0.3]");
    assert_eq!(high.name, "classification[t0.7]");
    assert_ne!(low.name, high.name);

    // Distinct identities keep RunComparator from joining the two cutoffs and reporting an incomparable directional delta on `f1` or any confusion-derived value.
    let base = make_run_result("base", "baseline", vec![low], vec![]);
    let target = make_run_result("target", "candidate", vec![high], vec![]);
    let diff = RunComparator::new().compare(&base, &target);
    assert!(
        diff.changes.is_empty(),
        "runs scored at different thresholds must not be joined and diffed"
    );
}

#[test]
fn fuzzy_match_identity_folds_threshold_for_comparison_safety() {
    let data = vec![
        binary_sample("fm01", "hello", "helo", 1.0),
        binary_sample("fm02", "world", "world", 1.0),
        binary_sample("fm03", "kitten", "sitting", 0.5),
    ];
    let low = fuzzy_match::<String>(0.5).compute(&data).unwrap();
    let high = fuzzy_match::<String>(0.9).compute(&data).unwrap();

    assert_eq!(low.name, "fuzzy_match[t0.5]");
    assert_eq!(high.name, "fuzzy_match[t0.9]");

    let base = make_run_result("base", "baseline", vec![low], vec![]);
    let target = make_run_result("target", "candidate", vec![high], vec![]);
    let diff = RunComparator::new().compare(&base, &target);
    assert!(
        diff.changes.is_empty(),
        "fuzzy runs scored at different thresholds must not be joined and diffed"
    );
}

#[test]
fn fuzzy_match_empty_input_keeps_threshold_provenance() {
    let empty: Vec<ScoredSample<String>> = Vec::new();
    let result = fuzzy_match::<String>(0.7).compute(&empty).unwrap();

    // The empty path must carry the same identity and provenance contract as a populated run, so an empty run never loses the configured threshold.
    assert_eq!(result.name, "fuzzy_match[t0.7]");
    assert!(result.values.is_empty());
    let detail = result
        .detail
        .expect("empty fuzzy run keeps threshold provenance");
    assert_eq!(detail["threshold"], 0.7);
}

#[test]
fn classification_rejects_non_finite_threshold() {
    let data = classification_data();
    for bad in [f64::NAN, f64::INFINITY, -0.1, 1.1] {
        let err = binary_classification("pos".to_string(), bad)
            .compute(&data)
            .expect_err("out-of-range threshold must be rejected");
        assert_eq!(err.code(), ErrorCode::InvalidInput);
    }
}

#[test]
fn fuzzy_match_rejects_non_finite_threshold() {
    let data = vec![binary_sample("fm01", "hello", "helo", 1.0)];
    let empty: Vec<ScoredSample<String>> = Vec::new();
    for bad in [f64::NAN, f64::INFINITY, -0.1, 1.1] {
        // Both the populated and empty-input paths must reject an invalid threshold
        // rather than emit a `tNaN` identity with `null` provenance.
        assert_eq!(
            fuzzy_match::<String>(bad)
                .compute(&data)
                .expect_err("out-of-range threshold must be rejected")
                .code(),
            ErrorCode::InvalidInput
        );
        assert_eq!(
            fuzzy_match::<String>(bad)
                .compute(&empty)
                .expect_err("out-of-range threshold must be rejected on the empty path")
                .code(),
            ErrorCode::InvalidInput
        );
    }
}

// ---------------------------------------------------------------------------
// 7. Report generation
// ---------------------------------------------------------------------------

fn sample_run_result() -> BenchRunResult {
    let metrics = vec![
        MetricResult {
            name: "accuracy".into(),
            value: 0.85,
            values: HashMap::new(),
            direction: MetricDirection::HigherIsBetter,
            detail: None,
        },
        MetricResult {
            name: "f1".into(),
            value: 0.82,
            values: {
                let mut m = HashMap::new();
                m.insert("precision".into(), 0.84);
                m.insert("recall".into(), 0.80);
                m
            },
            direction: MetricDirection::HigherIsBetter,
            detail: None,
        },
    ];
    let samples = vec![
        BenchSampleResult {
            id: "s1".into(),
            label: "pos".into(),
            predicted: "pos".into(),
            score: 0.9,
            correct: true,
            branch_scores: HashMap::new(),
            duration_ms: 5,
            error: String::new(),
        },
        BenchSampleResult {
            id: "s2".into(),
            label: "pos".into(),
            predicted: "neg".into(),
            score: 0.4,
            correct: false,
            branch_scores: HashMap::new(),
            duration_ms: 3,
            error: String::new(),
        },
        BenchSampleResult {
            id: "s3".into(),
            label: "neg".into(),
            predicted: "neg".into(),
            score: 0.2,
            correct: true,
            branch_scores: HashMap::new(),
            duration_ms: 4,
            error: String::new(),
        },
    ];
    make_run_result("run-golden-001", "v1.0", metrics, samples)
}

#[test]
fn golden_json_report() {
    let result = sample_run_result();
    let mut buf = Cursor::new(Vec::new());
    let reporter = JsonReporter;
    reporter.generate(&mut buf, &result).unwrap();
    let output = String::from_utf8(buf.into_inner()).unwrap();
    // Parse and re-serialize for deterministic key order
    let parsed: serde_json::Value = serde_json::from_str(&output).unwrap();
    insta::assert_json_snapshot!("json_report", parsed);
}

#[test]
fn golden_markdown_report() {
    let result = sample_run_result();
    let mut buf = Cursor::new(Vec::new());
    let reporter = MarkdownReporter;
    reporter.generate(&mut buf, &result).unwrap();
    let output = String::from_utf8(buf.into_inner()).unwrap();
    insta::assert_snapshot!("markdown_report", output);
}

// ---------------------------------------------------------------------------
// 8. Comparison / regression detection
// ---------------------------------------------------------------------------

#[test]
fn golden_run_comparison() {
    let base_metrics = vec![
        MetricResult {
            name: "f1".into(),
            value: 0.80,
            values: {
                let mut m = HashMap::new();
                m.insert("precision".into(), 0.82);
                m.insert("recall".into(), 0.78);
                m
            },
            direction: MetricDirection::HigherIsBetter,
            detail: None,
        },
        MetricResult {
            name: "accuracy".into(),
            value: 0.85,
            values: HashMap::new(),
            direction: MetricDirection::HigherIsBetter,
            detail: None,
        },
    ];
    let base_samples = vec![
        BenchSampleResult {
            id: "s1".into(),
            label: "pos".into(),
            predicted: "pos".into(),
            score: 0.9,
            correct: true,
            branch_scores: HashMap::new(),
            duration_ms: 5,
            error: String::new(),
        },
        BenchSampleResult {
            id: "s2".into(),
            label: "pos".into(),
            predicted: "neg".into(),
            score: 0.4,
            correct: false,
            branch_scores: HashMap::new(),
            duration_ms: 3,
            error: String::new(),
        },
        BenchSampleResult {
            id: "s3".into(),
            label: "neg".into(),
            predicted: "neg".into(),
            score: 0.2,
            correct: true,
            branch_scores: HashMap::new(),
            duration_ms: 4,
            error: String::new(),
        },
    ];
    let base = make_run_result("base-001", "baseline", base_metrics, base_samples);

    // Target: F1 improved, accuracy regressed; sample s2 fixed, s3 regressed
    let target_metrics = vec![
        MetricResult {
            name: "f1".into(),
            value: 0.84,
            values: {
                let mut m = HashMap::new();
                m.insert("precision".into(), 0.86);
                m.insert("recall".into(), 0.82);
                m
            },
            direction: MetricDirection::HigherIsBetter,
            detail: None,
        },
        MetricResult {
            name: "accuracy".into(),
            value: 0.83,
            values: HashMap::new(),
            direction: MetricDirection::HigherIsBetter,
            detail: None,
        },
    ];
    let target_samples = vec![
        BenchSampleResult {
            id: "s1".into(),
            label: "pos".into(),
            predicted: "pos".into(),
            score: 0.92,
            correct: true,
            branch_scores: HashMap::new(),
            duration_ms: 5,
            error: String::new(),
        },
        BenchSampleResult {
            id: "s2".into(),
            label: "pos".into(),
            predicted: "pos".into(),
            score: 0.6,
            correct: true, // was false → fixed
            branch_scores: HashMap::new(),
            duration_ms: 3,
            error: String::new(),
        },
        BenchSampleResult {
            id: "s3".into(),
            label: "neg".into(),
            predicted: "pos".into(),
            score: 0.55,
            correct: false, // was true → regressed
            branch_scores: HashMap::new(),
            duration_ms: 4,
            error: String::new(),
        },
    ];
    let target = make_run_result("target-002", "candidate", target_metrics, target_samples);

    let comparator = RunComparator::new().with_threshold(0.01);
    let mut diff = comparator.compare(&base, &target);
    // Sort changes by name for deterministic snapshot output
    diff.changes.sort_by(|a, b| a.name.cmp(&b.name));

    insta::assert_json_snapshot!("run_comparison", &diff);
    insta::assert_snapshot!("run_comparison_summary", diff.summary());
}

// ---------------------------------------------------------------------------
// 9. Metric suite (combined)
// ---------------------------------------------------------------------------

#[test]
fn golden_suite_binary() {
    let data = classification_data();
    let mut suite = Suite::new(vec![]);
    suite.add(binary_classification("pos".to_string(), 0.5));
    suite.add(exact_match::<String>());

    let results = suite.compute(&data).unwrap();
    let stable: Vec<serde_json::Value> = results.iter().map(stable_metric).collect();
    insta::assert_json_snapshot!("suite_binary", stable);
}
