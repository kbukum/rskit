//! Criterion benchmarks for rskit-bench metric computations.

use criterion::{BenchmarkId, Criterion, criterion_group, criterion_main};
use std::collections::HashMap;
use std::hint::black_box;
use std::io::Cursor;

use rskit_bench::metric::{
    Metric, binary_classification, mae, mse, r_squared, rmse, threshold_sweep,
};
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
    let mut r = BenchRunResult::default();
    r.id = "bench-run-001".into();
    r.schema = "https://gokit.dev/bench/v1/schema.json".into();
    r.version = "1.0".into();
    r.timestamp = "2025-01-15T12:00:00Z".into();
    r.tag = "bench".into();
    r.duration_ms = 100;
    r.dataset = DatasetInfo {
        name: "bench-dataset".into(),
        version: "1.0".into(),
        sample_count: n_samples,
        label_distribution: HashMap::new(),
    };
    r.metrics = metrics;
    r.samples = samples;
    r
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
        let data = gen_classification_samples(size);
        let metric = threshold_sweep("pos".to_string(), Some(vec![0.5]));
        group.bench_with_input(
            BenchmarkId::new("threshold_sweep", size),
            &data,
            |b, data| {
                b.iter(|| metric.compute(black_box(data)));
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
