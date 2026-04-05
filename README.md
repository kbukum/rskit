# rskit

A production-grade Rust toolkit for building scalable, resilient services —
the spiritual Rust twin of [gokit](https://github.com/kbukum/gokit).

[![CI](https://github.com/kbukum/rskit/actions/workflows/ci.yml/badge.svg)](https://github.com/kbukum/rskit/actions/workflows/ci.yml)
[![Crates.io](https://img.shields.io/crates/v/rskit.svg)](https://crates.io/crates/rskit)
[![docs.rs](https://img.shields.io/docsrs/rskit)](https://docs.rs/rskit)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![MSRV: 1.85](https://img.shields.io/badge/MSRV-1.85-orange.svg)](rust-toolchain.toml)

---

## Overview

rskit provides composable building blocks for service development in Rust,
covering the same problem space as gokit but using idiomatic Rust patterns:

| Concern | rskit crate | Key design choice vs. gokit |
|---|---|---|
| Structured errors | `rskit-errors` | `ErrorCode` enum (exhaustive match) instead of string constants |
| Config loading | `rskit-config` | Layered TOML → `.env` → env vars with `validator` |
| Observability | `rskit-logging` | `tracing` subscriber, one-shot setup, no global registry |
| Service lifecycle | `rskit-bootstrap` | Typestate `App<S,C>` — lifecycle ordering enforced at compile time |
| Resilience | `rskit-resilience` | `governor` rate limiter, `parking_lot` CB, Tower layers |
| Async I/O patterns | `rskit-provider` | Four interaction traits bridging `tower::Service` |
| Stream processing | `rskit-pipeline` | `futures::Stream` extension trait with 13 operators |
| Worker pool | `rskit-worker` | `JoinSet` + `Semaphore`, event relay, panic detection |
| gRPC transport | `rskit-server` | `tonic` server as a `Component` |

---

## Quick Start

Add the facade crate to your service:

```toml
[dependencies]
rskit = "0.1"
tokio = { version = "1", features = ["full"] }
```

Or pick only what you need:

```toml
[dependencies]
rskit-errors     = "0.1"
rskit-resilience = "0.1"
rskit-worker     = "0.1"
```

### Hello, lifecycle

```rust
use rskit_bootstrap::{App, AppBuilder, Component, Health};
use rskit_config::ServiceConfig;
use rskit_errors::AppResult;
use std::sync::Arc;

#[tokio::main]
async fn main() -> AppResult<()> {
    let config = MyConfig::default();

    AppBuilder::new(config)
        .build()?
        .on_start(|cfg, _cancel| async move {
            println!("Starting {}", cfg.service_config().name);
            Ok(())
        })
        .on_ready(|_cfg, _cancel| async move {
            println!("Ready.");
            Ok(())
        })
        .run()
        .await
}
```

### Resilient HTTP call

```rust
use rskit_resilience::{CircuitBreaker, CbConfig, RetryPolicy};
use std::time::Duration;

let cb = CircuitBreaker::new(CbConfig::default());
let retry = RetryPolicy::builder()
    .max_attempts(3)
    .initial_backoff(Duration::from_millis(100))
    .build();

let result = retry.execute(|| async {
    cb.execute(|| async { call_external_service().await }).await
}).await?;
```

### Stream pipeline

```rust
use rskit_pipeline::{RskitStreamExt, from_slice};
use futures::StreamExt;

let results = from_slice(vec![1u32, 2, 3, 4, 5])
    .rfilter(|&n| async move { n % 2 == 0 })
    .rmap(|n| async move { Ok(n * 10) })
    .collect::<Vec<_>>()
    .await;
// [Ok(20), Ok(40)]
```

### Worker pool

```rust
use rskit_worker::{Handler, Pool, PoolConfig, Event};
use rskit_errors::AppResult;
use tokio::sync::mpsc;
use tokio_util::sync::CancellationToken;
use std::sync::Arc;

struct MyHandler;

#[async_trait::async_trait]
impl Handler<String, String> for MyHandler {
    async fn handle(
        &self,
        input: String,
        emit: mpsc::Sender<Event<String>>,
        _cancel: CancellationToken,
    ) -> AppResult<String> {
        Ok(input.to_uppercase())
    }
}

let pool = Pool::new(Arc::new(MyHandler), PoolConfig::new("demo"));
let handle = pool.submit("hello".to_string()).await?;
let result = handle.result().await?;
assert_eq!(result, "HELLO");
```

---

### Crate Map

| Phase | Crate | Description |
|-------|-------|-------------|
| Core | `rskit` | Production Rust toolkit — modular facade for rskit-* crates |
| Core | `rskit-errors` | Structured application error types with HTTP/gRPC status mapping |
| Core | `rskit-config` | TOML + environment variable configuration loading with validation |
| Core | `rskit-logging` | Structured logging setup using tracing — JSON in production, pretty in dev |
| Core | `rskit-bootstrap` | Application lifecycle orchestration: typestate App, Component registry, hooks |
| Core | `rskit-provider` | Provider traits (request-response, stream, sink, duplex) with tower middleware |
| Core | `rskit-pipeline` | Composable async data pipelines via futures::Stream extension operators |
| Core | `rskit-resilience` | Fault-tolerance: retry, circuit breaker, bulkhead, rate limiter + tower layers |
| Core | `rskit-worker` | Task worker pool with JoinSet, typed events, and provider bridges |
| Core | `rskit-server` | tonic gRPC server bootstrap as a lifecycle-managed Component |
| Foundation | `rskit-validation` | Fluent field-level validator that collects errors and converts to AppError |
| Foundation | `rskit-http` | Axum HTTP server with graceful shutdown, CORS, request-ID, and Component lifecycle |
| Foundation | `rskit-di` | Lightweight Arc-based runtime dependency injection container |
| Foundation | `rskit-auth` | JWT, OIDC, password hashing, and request-context auth helpers |
| Adapters | `rskit-database` | sqlx-based async database pool with repository pattern and slow-query logging |
| Adapters | `rskit-cache` | Redis client with typed store, connection management, and Component lifecycle |
| Adapters | `rskit-messaging` | Message broker abstractions with Kafka support |
| Platform | `rskit-observability` | OpenTelemetry tracing, metrics, and context propagation |
| Platform | `rskit-authz` | RBAC and ABAC authorization engine |
| Platform | `rskit-discovery` | Service discovery with load balancing strategies |
| Specialist | `rskit-testutil` | Test utilities, mock providers, and assertion helpers |
| Specialist | `rskit-sse` | Server-Sent Events bus with axum integration |
| Specialist | `rskit-dag` | DAG task orchestrator with parallel execution |
| Specialist | `rskit-llm` | LLM provider abstractions for OpenAI and Anthropic |
| Specialist | `rskit-embedding` | Embedding provider abstractions for vector search |
| Specialist | `rskit-inference` | Inference provider abstractions for LLM chat completions |
| Specialist | `rskit-vector-store` | Vector store abstractions with Qdrant and in-memory implementations |
| Media & File | `rskit-file` | File I/O, storage backends, temp files, and MIME detection |
| Media & File | `rskit-media` | Media types, codec/format registry, pipeline builder, and processing traits |
| Media & File | `rskit-media-ffmpeg` | FFmpeg CLI backend for video/audio processing |
| Media & File | `rskit-media-image` | Native image processing backend using the image crate |
| CLI & Data | `rskit-cli` | CLI framework: progress bars, structured output, signal handling |
| CLI & Data | `rskit-dataset` | Dataset collection framework: source, transform, target, collector |
| CLI & Data | `rskit-bench` | ML benchmarking framework: evaluators, metrics, reports, visualization |

### Dependency graph

```
rskit-errors
rskit-config       → rskit-errors
rskit-logging      → rskit-config
rskit-resilience   → rskit-errors
rskit-provider     → rskit-errors, rskit-resilience
rskit-pipeline     → rskit-errors
rskit-bootstrap    → rskit-errors, rskit-config, rskit-logging
rskit-worker       → rskit-errors, rskit-provider, rskit-pipeline, rskit-resilience
rskit-server       → rskit-bootstrap, rskit-errors, rskit-config, rskit-resilience
rskit (facade)     → all above
```

No circular dependencies. `rskit-bootstrap` intentionally does **not** depend on
`rskit-provider` or `rskit-worker` — components are registered as `Arc<dyn Component>`,
keeping the core lifecycle thin.

---

## Feature Highlights

### Errors — `rskit-errors`

```rust
// Typed error codes — exhaustive match, no string typos
match err.code() {
    ErrorCode::NotFound      => 404,
    ErrorCode::Unauthorized  => 401,
    ErrorCode::RateLimited   => 429,
    _                        => 500,
}

// Fluent builder
let err = AppError::not_found("user", user_id)
    .with_detail("tenant", tenant_id)
    .with_cause(db_error);

// tonic interop
let status: tonic::Status = err.into();
```

### Resilience — `rskit-resilience`

```rust
// Circuit breaker
let cb = CircuitBreaker::new(CbConfig {
    max_failures: 5,
    timeout: Duration::from_secs(30),
    ..Default::default()
});

// Retry with exponential backoff + jitter
let policy = RetryPolicy::builder()
    .max_attempts(4)
    .initial_backoff(Duration::from_millis(50))
    .backoff_factor(2.0)
    .with_jitter(true)
    .build();

// Tower integration — wrap any tower::Service
use tower::ServiceBuilder;
use rskit_resilience::{CircuitBreakerLayer, RetryLayer};

let svc = ServiceBuilder::new()
    .layer(CircuitBreakerLayer(cb))
    .layer(RetryLayer(policy))
    .service(my_service);
```

### Pipeline — `rskit-pipeline`

All operators are lazy and non-allocating where possible.

| Operator | Description |
|---|---|
| `rmap` / `rflatmap` | Async map / flat-map |
| `rfilter` | Async predicate filter |
| `rtap` | Side-effect without transforming |
| `rreduce` | Fold to a single value |
| `rparallel` | Bounded concurrent execution |
| `rfan_out` | Broadcast item to N async functions |
| `rbatch` | Collect N items into a `Vec` |
| `rdebounce` | Suppress rapid bursts; emit last after quiet period |
| `rthrottle` | Emit at most once per interval |
| `rtumbling_window` | Fixed non-overlapping time windows |
| `rsliding_window` | Overlapping time windows |

### Config loading order

```
1. TOML file (optional)            ← lowest priority
2. .env file (optional, dotenvy)
3. APP__SECTION__KEY env vars       ← highest priority
```

```rust
#[derive(Deserialize, Validate, AppConfig)]
struct Config {
    service: ServiceConfig,
    #[validate(range(min = 1, max = 65535))]
    port: u16,
}

let cfg: Config = ConfigLoader::new()
    .with_config_file("config/app.toml")
    .with_env_prefix("MYAPP")
    .load()?;
```

---

## Design Decisions

| Decision | Rationale |
|---|---|
| `ErrorCode` as enum, not strings | Exhaustive pattern matching, derives `Hash`/`Copy`, no typos |
| `tower::Layer` for middleware | Industry standard, free tonic interop, composable |
| `futures::Stream` extension trait | Native async, tokio time interop, works with gRPC streaming |
| `governor` for rate limiting | Production-grade, injectable clock for deterministic tests |
| `parking_lot::Mutex` for circuit breaker | Non-poisoning, never held across `.await`, ~50% faster |
| `CancellationToken` for shutdown | Idiomatic Tokio cooperative cancellation |
| Typestate `App<S, C>` | Compile-time lifecycle ordering — can't call `run` before `build` |
| No DI container | Rust's type system makes runtime DI stringly-typed without benefit |
| `JoinSet` + `Semaphore` for worker pool | Idiomatic Tokio, panic detection via `JoinError`, zero boilerplate |
| mpsc → broadcast relay in pool | Allows `T: Clone` without `T: Sync`, scales to N subscribers |

---

## Comparison with gokit

rskit mirrors gokit's package structure and lifecycle philosophy. Key differences:

| gokit | rskit | Why |
|---|---|---|
| `ErrorCode` as string constants | `ErrorCode` enum | Exhaustive match, compile-time safety |
| Custom `Middleware[I,O]` chain | `tower::Layer` | Industry standard |
| Custom pull-based `Iterator[T]` | `futures::Stream` extension | Native async |
| Custom token bucket | `governor` | Production-grade, testable |
| `sync.Mutex` in CB | `parking_lot::Mutex` | Non-poisoning |
| `context.Context` cancellation | `CancellationToken` | Rust-idiomatic |
| Goroutine-per-worker pool | `JoinSet` pool | Idiomatic Tokio |
| Runtime DI container | `rskit-di` container | Lightweight `Arc`-based, opt-in |

All 34 workspace crates are implemented and included in v0.1, covering DI,
observability (OTEL), service discovery, database, Redis, and Kafka adapters.

---

## Minimum Supported Rust Version (MSRV)

**1.85** — enforced by `rust-toolchain.toml` and the CI matrix.

MSRV bumps are treated as minor version changes and documented in `CHANGELOG.md`.

---

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for development setup, testing conventions,
commit style, and the PR process.

---

## License

rskit is distributed under the terms of the [MIT License](LICENSE).

Copyright (c) 2024 kbukum contributors.
