# rskit-bench — ML Benchmarking Framework

ML benchmarking framework: evaluators, metrics, reports, and visualization.

[![CI](https://github.com/kbukum/rskit/actions/workflows/ci.yml/badge.svg)](https://github.com/kbukum/rskit/actions/workflows/ci.yml) [![crates.io](https://img.shields.io/crates/v/rskit-bench.svg)](https://crates.io/crates/rskit-bench) [![docs.rs](https://docs.rs/rskit-bench/badge.svg)](https://docs.rs/rskit-bench) [![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/kbukum/rskit/blob/main/LICENSE) [![MSRV: 1.97](https://img.shields.io/badge/MSRV-1.97-orange.svg)](https://www.rust-lang.org/)

## Features

- `Evaluator<L>` trait + `EvaluatorFunc` closure wrapper + `FromProvider` adapter
- `BenchRunner` — orchestrates evaluation with configurable concurrency
- Classification metrics: accuracy, precision, recall, F1, AUC-ROC
- Async metrics: `AsyncMetric` trait for I/O-backed scoring, awaited after the synchronous metrics; `semantic_similarity` scores predictions against references by embedding-cosine similarity through an injected `rskit-embedding` provider, and `llm_judge` grades them with an injected `rskit-llm` provider and a versioned prompt — both with a per-call timeout and typed failure paths
- Multi-branch comparison for A/B model testing
- Reports: JSON, CSV, Markdown, JUnit, Vega-Lite visualizations
- `FileRunStorage` for persistent result storage and regression detection
- ROC curves, confusion matrices, score distribution charts
- `BenchClock` / `FixedClock` for deterministic timestamps and durations in tests
- Reproducible runs: every result carries `RunProvenance` (seed, source commit, tool/host identity, order-independent dataset hash, and — when an `llm_judge` metric is present — the judge provider, model, and prompt identity), gathered through an injected `ProvenanceProbe`

Benchmark orchestration accepts injected clock, storage, and provenance implementations. Production CLIs can choose `SystemClock`, `FileRunStorage`, and the default `SystemProvenanceProbe` (which reads host/os/arch from the standard library and the commit from CI environment variables such as `GITHUB_SHA`). Tests and reproducible harnesses inject `FixedClock`, an in-memory or tempdir storage, and a `FixedProvenanceProbe` so a run's provenance is byte-for-byte deterministic. Set the run seed with `RunOptions::with_seed`; `RunOptions::seeded_rng` derives a reproducible RNG from it.

## Metrics

A `Suite` holds two kinds of metrics. Synchronous `Metric`s are pure and deterministic — they compute from scored samples with no I/O. Asynchronous `AsyncMetric`s back their scoring with external work, such as an embedding or LLM provider. The runner evaluates synchronous metrics first, then awaits asynchronous ones, appending results in suite order so a run's metric sequence stays reproducible.

### Semantic similarity

`semantic_similarity` embeds predictions and references through an injected provider, then reports average cosine similarity and threshold match rate. Model and threshold identify the metric and its provenance, keeping incompatible runs separate.

Configure bounded batches with `with_batch_size` and a resilience policy with a per-call timeout. Provider failures, invalid indices, dimension mismatches, and empty input produce typed results, not fabricated scores. For offline tests, inject `InMemoryProvider` or `FakeEmbeddingProvider` (`embedding` feature). Use `as_sync` for precomputed async results.

Using it takes an embedding provider and model alongside `rskit-bench`:

```toml
[dependencies]
rskit-bench = "0.2.0-alpha.5"
rskit-embedding = "0.2.0-alpha.4"
rskit-ai = "0.2.0-alpha.5"
```

```rust
use rskit_ai::{Capabilities, Model, Provider};
use rskit_bench::{Suite, semantic_similarity};
use rskit_embedding::InMemoryProvider;
use std::sync::Arc;
use std::time::Duration;

// Inject any embedding provider; swap `InMemoryProvider` for a real adapter.
let provider = Arc::new(InMemoryProvider::new(384));
let model = Model {
    name: "text-embedding-3-small".into(),
    provider: Provider::OpenAI,
    version: None,
    capabilities: Capabilities::default(),
};

// Configure the metric's failure bounds, then register it on a suite.
let metric = semantic_similarity::<String>(provider, model)
    .with_threshold(0.85)
    .with_timeout(Duration::from_secs(10))
    .with_batch_size(32);

let mut suite = Suite::<String>::new(Vec::new());
suite.add_async(Arc::new(metric));

// `compute_all` runs synchronous metrics first, then awaits the async ones.
let results = suite.compute_all(&scored_samples).await?;
```

### LLM judge

`llm_judge` grades prediction/reference pairs through an injected LLM and versioned `JudgePrompt`. It reports average score and threshold pass rate. Provider, requested/resolved model, prompt identity, and threshold keep comparisons tied to the scoring configuration; rerun after model or prompt changes.

Calls have bounded concurrency and a per-call resilience timeout. Output is limited by `with_max_tokens` (256 by default) and a fixed byte cap. Abnormal completions and malformed, incomplete, or out-of-range JSON verdicts return typed errors.

Validation proves `JudgeVerdict { score, rationale }` shape, not resistance to prompt injection. Reference and prediction text remain data under the system rubric; well-formed hostile verdicts can still parse. Use `FakeLlmProvider` (`llm` feature) for deterministic offline tests.

```rust
use rskit_bench::{JudgePrompt, Suite, llm_judge};
use std::sync::Arc;
use std::time::Duration;

// Inject any `rskit_llm::Provider`; the built-in prompt ships a versioned rubric.
let metric = llm_judge::<String>(judge_provider, "gpt-4o-mini", JudgePrompt::default())
    .with_threshold(0.7)
    .with_timeout(Duration::from_secs(20))
    .with_concurrency(4);

let mut suite = Suite::<String>::new(Vec::new());
suite.add_async(Arc::new(metric));
let results = suite.compute_all(&scored_samples).await?;
```

## Usage

```toml
[dependencies]
rskit-bench = "0.2.0-alpha.5"
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
