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

`semantic_similarity` is an `AsyncMetric` that scores each prediction against its reference by embedding both texts through an injected `rskit-embedding` provider and taking their cosine similarity, rather than comparing surface strings. It reports the average similarity and a match rate at a configurable threshold; the threshold is folded into the metric identity (so a match rate computed under one cutoff is never compared against one computed under another) and recorded in provenance rather than emitted as a directional value. Samples are embedded in bounded batches (`with_batch_size`), each provider call routed through an injected `rskit-resilience` policy (a per-call timeout by default), so a large run neither exceeds provider batch limits nor rides on a single dataset-wide deadline. Provider errors, dimension mismatches, out-of-range provider indices, and empty input yield typed results rather than panics or fabricated scores. The metric name embeds the embedding model's identity together with the configured threshold, and the result records both in provenance, so runs scored with incompatible models or thresholds are never compared as if equivalent. Inject a deterministic provider (for example `rskit_embedding::InMemoryProvider`, or `rskit_testutil::FakeEmbeddingProvider` with the `embedding` feature) to keep tests offline. A resolved `AsyncMetric` result can also be surfaced through the synchronous path with `as_sync`, for callers that precompute scores during evaluation.

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

`llm_judge` is an `AsyncMetric` that grades each prediction against its reference with an injected `rskit-llm` provider and a **versioned** `JudgePrompt` built on the canonical `rskit-ai` prompt template. It reports the average score and a pass rate at a configurable threshold; the threshold is folded into the metric identity, so a pass rate computed under one cutoff is never compared against one computed under another. Each provider call is routed through an injected `rskit-resilience` policy (a per-call timeout by default) and capped to a bounded output (`with_max_tokens`, default 256), and the reply is additionally rejected if it exceeds a fixed byte bound or if the completion did not finish normally (truncated by the token limit, blocked by a content filter, or cancelled) — so an untrusted judge cannot force an unbounded parse or pass off an incomplete generation as a score. Calls are issued with bounded concurrency (`with_concurrency`) so a large run applies backpressure to the judge rather than flooding it. The model's reply is treated as **untrusted**: the judge requests a JSON object and parses it into a validated `JudgeVerdict { score, rationale }`. A malformed reply, an out-of-range score, a missing field, or non-JSON surrounding prose yields a typed error — never a fabricated success-shaped score and never a panic. This parsing enforces the verdict *shape* at the trust boundary; it is not a prompt-injection detector (a well-formed `{"score": 1}` still parses), so the injection defense is structural: the reference and prediction texts are embedded as data behind a system prompt that instructs the judge to treat them as data, not instructions. The judge provider, model (and the resolved model the provider reports, when it differs), and prompt identity are recorded in the result and lifted into `RunProvenance`, so scores are reproducible and never silently compared across prompt or model revisions; gate any prompt or model change on a re-run. Inject `rskit_testutil::FakeLlmProvider` (with the `llm` feature) to keep tests offline and deterministic.

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
