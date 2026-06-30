# rskit-bench — ML Benchmarking Framework

ML benchmarking framework: evaluators, metrics, reports, and visualization.

[![CI](https://github.com/kbukum/rskit/actions/workflows/ci.yml/badge.svg)](https://github.com/kbukum/rskit/actions/workflows/ci.yml) [![crates.io](https://img.shields.io/crates/v/rskit-bench.svg)](https://crates.io/crates/rskit-bench) [![docs.rs](https://docs.rs/rskit-bench/badge.svg)](https://docs.rs/rskit-bench) [![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/kbukum/rskit/blob/main/LICENSE) [![MSRV: 1.91](https://img.shields.io/badge/MSRV-1.91-orange.svg)](https://www.rust-lang.org/)

## Features

- `Evaluator<L>` trait + `EvaluatorFunc` closure wrapper + `FromProvider` adapter
- `BenchRunner` — orchestrates evaluation with configurable concurrency
- Classification metrics: accuracy, precision, recall, F1, AUC-ROC
- Multi-branch comparison for A/B model testing
- Reports: JSON, CSV, Markdown, JUnit, Vega-Lite visualizations
- `FileRunStorage` for persistent result storage and regression detection
- ROC curves, confusion matrices, score distribution charts
- `BenchClock` / `FixedClock` for deterministic timestamps and durations in tests

Benchmark orchestration accepts injected clock and storage implementations. Production CLIs can choose `SystemClock` and `FileRunStorage`; tests and reproducible harnesses should inject `FixedClock` and an in-memory or tempdir storage implementation.

## Usage

```toml
[dependencies]
rskit-bench = "0.2.0-alpha.1"
```

```rust
use rskit_bench::{BenchRunner, EvaluatorFunc, FixedClock, Prediction, RunOptions};
use rskit_errors::AppResult;
use std::sync::Arc;

let eval = EvaluatorFunc::new("heuristic", |input: Vec<u8>| {
    Box::pin(async move {
        let score = input.iter().map(|&b| b as f64).sum::<f64>() / 2560.0;
        Ok(Prediction {
            label: if score > 0.5 { "positive" } else { "negative" }.into(),
            score,
            confidence: 0.8,
            metadata: Default::default(),
        })
    })
});

println!("Evaluator: {}", eval.name());
let runner = BenchRunner::new()
    .register("heuristic", Box::new(eval), 0)
    .with_clock(Arc::new(FixedClock::new(1_700_000_000, 0)));
// Load samples and run with RunOptions
```

## See Also

[Main repository README](https://github.com/kbukum/rskit)
