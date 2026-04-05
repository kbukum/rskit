# rskit-bench — ML Benchmarking Framework

ML benchmarking framework: evaluators, metrics, reports, and visualization.

[![CI](https://github.com/kbukum/rskit/actions/workflows/ci.yml/badge.svg)](https://github.com/kbukum/rskit/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/rskit-bench.svg)](https://crates.io/crates/rskit-bench)
[![docs.rs](https://docs.rs/rskit-bench/badge.svg)](https://docs.rs/rskit-bench)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![MSRV: 1.85](https://img.shields.io/badge/MSRV-1.85-orange.svg)](https://blog.rust-lang.org/2025/02/20/Rust-1.85.0.html)

## Features

- `Evaluator<L>` trait + `EvaluatorFunc` closure wrapper + `FromProvider` adapter
- `BenchRunner` — orchestrates evaluation with configurable concurrency
- Classification metrics: accuracy, precision, recall, F1, AUC-ROC
- Multi-branch comparison for A/B model testing
- Reports: JSON, CSV, Markdown, JUnit, Vega-Lite visualizations
- `FileRunStorage` for persistent result storage and regression detection
- ROC curves, confusion matrices, score distribution charts

## Usage

```toml
[dependencies]
rskit-bench = "0.1"
```

```rust
use rskit_bench::{EvaluatorFunc, Prediction, BenchRunner, RunOptions};
use rskit_errors::AppResult;

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
// Add to BenchRunner, load samples, and run
```

## See Also

[Main repository README](https://github.com/kbukum/rskit)
