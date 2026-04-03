//! Criterion benchmarks for rskit-bench metric computations.

use criterion::{BenchmarkId, Criterion, black_box, criterion_group, criterion_main};
use std::collections::HashMap;
use std::io::Cursor;

use rskit_bench::metric::{Metric, binary_classification, mae, mse, r_squared, rmse};
use rskit_bench::metrics::compute_metrics;
use rskit_bench::report_gen::{JsonReporter, MarkdownReporter, Reporter};
use rskit_bench::result::{BenchRunResult, BenchSampleResult, DatasetInfo, MetricResult};
use rskit_bench::types::{BenchSample, Prediction, ScoredSample};

// ---------------------------------------------------------------------------
// Data generators
// ---------------------------------------------------------------------------

fn gen_classification_samples(n: usize) -> Vec<ScoredSample<String>> {
    (0..n)
        .map(|i| {
            let is_positive = i % 3 != 0;
            let score = if is_positive {
                0.5 + 0.5 * ((i as f64) / n as f64)
            } else {
                0.5 * ((i as f64) / n as f64)
            };
            let label = if is_positive { "pos" } else { "neg" };
            let pred = if score >= 0.5 { "pos" } else { "neg" };
            ScoredSample {
                sample: BenchSample {
                    id: format!("s{i}"),
                    input: vec![],
                    label: label.into(),
                    source: String::new(),
                    metadata: HashMap::new(),
                },
                prediction: Prediction {
                    sample_id: format!("s{i}"),
                    label: pred.into(),
                    score,
                    scores: HashMap::new(),
                    metadata: HashMap::new(),
                },
            }
        })
        .collect()
}

fn gen_regression_samples(n: usize) -> Vec<ScoredSample<f64>> {
    (0..n)
        .map(|i| {
            let actual = (i as f64) * 0.1;
            let predicted = actual + 0.05 * ((i % 7) as f64 - 3.0);
            ScoredSample {
                sample: BenchSample {
                    id: format!("r{i}"),
                    input: vec![],
                    label: actual,
                    source: String::new(),
                    metadata: HashMap::new(),
                },
                prediction: Prediction {
                    sample_id: format!("r{i}"),
                    label: predicted,
                    score: predicted,
                    scores: HashMap::new(),
                    metadata: HashMap::new(),
                },
            }
        })
        .collect()
}

fn gen_scored_arrays(n: usize) -> (Vec<f64>, Vec<bool>) {
    let scores: Vec<f64> = (0..n).map(|i| (i as f64) / (n as f64)).collect();
    let labels: Vec<bool> = (0..n).map(|i| i % 3 != 0).collect();
    (scores, labels)
}

fn make_bench_run_result(n_samples: usize) -> BenchRunResult {
    let metrics = vec![
        MetricResult {
            name: "accuracy".into(),
            value: 0.85,
            values: HashMap::new(),
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
            detail: None,
        },
    ];
    let samples: Vec<BenchSampleResult> = (0..n_samples)
        .map(|i| BenchSampleResult {
            id: format!("s{i}"),
            label: "pos".into(),
            predicted: if i % 5 == 0 {
                "neg".into()
            } else {
                "pos".into()
            },
            score: (i as f64) / (n_samples as f64),
            correct: i % 5 != 0,
            branch_scores: HashMap::new(),
            duration_ms: 2,
            error: String::new(),
        })
        .collect();
    BenchRunResult {
        id: "bench-run-001".into(),
        schema: "https://gokit.dev/bench/v1/schema.json".into(),
        version: "1.0".into(),
        timestamp: "2025-01-15T12:00:00Z".into(),
        tag: "bench".into(),
        duration_ms: 100,
        dataset: DatasetInfo {
            name: "bench-dataset".into(),
            version: "1.0".into(),
            sample_count: n_samples,
            label_distribution: HashMap::new(),
        },
        metrics,
        branches: HashMap::new(),
        samples,
        curves: HashMap::new(),
    }
}

// ---------------------------------------------------------------------------
// Benchmarks
// ---------------------------------------------------------------------------

fn bench_classification(c: &mut Criterion) {
    let mut group = c.benchmark_group("classification");
    for size in [1_000, 10_000] {
        let data = gen_classification_samples(size);
        let metric = binary_classification("pos".to_string(), 0.5);
        group.bench_with_input(
            BenchmarkId::new("binary_classification", size),
            &data,
            |b, data| {
                b.iter(|| metric.compute(black_box(data)));
            },
        );
    }
    group.finish();
}

fn bench_regression(c: &mut Criterion) {
    let mut group = c.benchmark_group("regression");
    for size in [1_000, 10_000] {
        let data = gen_regression_samples(size);
        let metrics: Vec<Box<dyn Metric<f64>>> = vec![mae(), mse(), rmse(), r_squared()];

        group.bench_with_input(
            BenchmarkId::new("all_regression_metrics", size),
            &data,
            |b, data| {
                b.iter(|| {
                    for m in &metrics {
                        m.compute(black_box(data));
                    }
                });
            },
        );
    }
    group.finish();
}

fn bench_threshold_metrics(c: &mut Criterion) {
    let mut group = c.benchmark_group("threshold_metrics");
    for size in [1_000, 10_000] {
        let (scores, labels) = gen_scored_arrays(size);
        group.bench_with_input(
            BenchmarkId::new("compute_metrics", size),
            &(scores.clone(), labels.clone()),
            |b, (s, l)| {
                b.iter(|| compute_metrics(black_box(s), black_box(l), 0.5));
            },
        );
    }
    group.finish();
}

fn bench_reports(c: &mut Criterion) {
    let mut group = c.benchmark_group("report_generation");
    let result = make_bench_run_result(100);

    group.bench_function("json_report", |b| {
        b.iter(|| {
            let mut buf = Cursor::new(Vec::with_capacity(4096));
            JsonReporter.generate(&mut buf, black_box(&result)).unwrap();
        });
    });

    group.bench_function("markdown_report", |b| {
        b.iter(|| {
            let mut buf = Cursor::new(Vec::with_capacity(4096));
            MarkdownReporter
                .generate(&mut buf, black_box(&result))
                .unwrap();
        });
    });

    group.finish();
}

criterion_group!(
    benches,
    bench_classification,
    bench_regression,
    bench_threshold_metrics,
    bench_reports,
);
criterion_main!(benches);
