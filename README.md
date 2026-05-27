# rskit

[![CI](https://github.com/kbukum/rskit/actions/workflows/ci.yml/badge.svg)](https://github.com/kbukum/rskit/actions/workflows/ci.yml)
[![Crates.io](https://img.shields.io/crates/v/rskit.svg)](https://crates.io/crates/rskit)
[![docs.rs](https://img.shields.io/docsrs/rskit)](https://docs.rs/rskit)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![MSRV: 1.85](https://img.shields.io/badge/MSRV-1.85-orange.svg)](rust-toolchain.toml)

**A production-grade Rust toolkit for building scalable, resilient services.** Structured errors, layered config, OpenTelemetry observability, typestate lifecycle, tower-based resilience, async pipelines, worker pools, security policy, and tonic gRPC — composable building blocks built on the standard Rust async ecosystem.

> **Status — pre-1.0.** Public surface is semver-stable per crate; breaking changes are documented in [`CHANGELOG.md`](CHANGELOG.md). See [`docs/policy/SEMVER.md`](docs/policy/SEMVER.md). MSRV bumps are minor version changes.

> **Sibling projects.** [**gokit**](https://github.com/kbukum/gokit) (Go) · rskit (Rust, this repo) · [**pykit**](https://github.com/kbukum/pykit) (Python). Public abstractions (`AppError`, `Component`, `Provider`, `Pipeline`, lifecycle hooks) are evaluated for parity across all three.

## Browse by Domain

Modules are organized into domains for scoped development. See [Module Index](docs/MODULE-INDEX.md) for the full breakdown.

| Domain | Focus | Quick check |
| ------ | ----- | ----------- |
| core | Foundation types, config, logging | `make check-core` |
| patterns | Component, provider, DI, hooks | `make check-patterns` |
| crosscutting | Observability, resilience, security | `make check-crosscutting` |
| composition | Bootstrap, pipeline, DAG, workers | `make check-composition` |
| transport | Server, HTTP, gRPC, SSE | `make check-transport` |
| auth | Authentication, authorization | `make check-auth` |
| data | Database, cache, storage, messaging | `make check-data` |
| ai | LLM, inference, agents, tools | `make check-ai` |
| media | Media processing, transcription | `make check-media` |
| infra | Workload, CLI, benchmarks, testing | `make check-infra` |

CI still runs the full workspace; on pull requests the `changes` job also publishes an `affected` domain list from `./scripts/affected-domains.sh` so future workflow steps can inspect the same domains developers target locally with `make check-<domain>`.

## Highlights

- **Cargo workspace** — facade crate (`rskit`) + 40+ independent `rskit-*` sub-crates. Add only what you need.
- **Idiomatic Rust** — `tower::Layer` middleware, `futures::Stream` extensions, `parking_lot` non-poisoning mutexes, `CancellationToken` cooperative shutdown, `JoinSet` worker pools.
- **Compile-time lifecycle safety** — typestate `App<S, C>` makes invalid lifecycle transitions impossible to write.
- **Production resilience** — `governor` rate limiter, circuit breaker, retry with backoff + jitter, bulkhead — all available as `tower::Layer`.
- **Typed errors** — `ErrorCode` enum (exhaustive match), RFC 9457 problem details, and lightweight HTTP status metadata; gRPC mapping lives in `rskit-grpc`.
- **Sibling parity** — APIs mirror [gokit](https://github.com/kbukum/gokit) (Go) and [pykit](https://github.com/kbukum/pykit) (Python). See [`docs/DESIGN.md`](docs/DESIGN.md) for cross-language design notes.

## Install

```toml
# Facade — re-exports all rskit-* crates
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

Requires **Rust 1.85+** (enforced by `rust-toolchain.toml`).

## Quickstart

```rust
use rskit_bootstrap::AppBuilder;
use rskit_config::ServiceConfig;
use rskit_errors::AppResult;

#[tokio::main]
async fn main() -> AppResult<()> {
    let config = MyConfig::default();

    AppBuilder::new(config)
        .build()?
        .before_start(|_cancel| async move {
            println!("Starting.");
            Ok(())
        })
        .after_start(|_cancel| async move {
            println!("Ready.");
            Ok(())
        })
        .run()
        .await
}
```

More examples (resilience, pipelines, workers, tower layers, …) → [`docs/EXAMPLES.md`](docs/EXAMPLES.md). Full crate list → [`docs/PACKAGES.md`](docs/PACKAGES.md).

## Documentation

| Topic | Link |
|---|---|
| All crates | [`docs/PACKAGES.md`](docs/PACKAGES.md) |
| Usage examples | [`docs/EXAMPLES.md`](docs/EXAMPLES.md) |
| Design decisions & gokit comparison | [`docs/DESIGN.md`](docs/DESIGN.md) |
| Security model | [`docs/security-model.md`](docs/security-model.md) |
| Architecture decisions | [`docs/adr/`](docs/adr/) |
| Versioning & releases | [`docs/VERSIONING.md`](docs/VERSIONING.md) · [`docs/RELEASING.md`](docs/RELEASING.md) |
| Semver & deprecation policy | [`docs/policy/SEMVER.md`](docs/policy/SEMVER.md) · [`docs/policy/DEPRECATION.md`](docs/policy/DEPRECATION.md) |
| Cross-crate integration | [`INTEGRATION.md`](INTEGRATION.md) |
| Per-crate API docs | [docs.rs/rskit](https://docs.rs/rskit) |

## Contributing

We welcome contributions. See [`CONTRIBUTING.md`](CONTRIBUTING.md) for setup, testing conventions, commit style, and the PR process. By participating you agree to the [Code of Conduct](CODE_OF_CONDUCT.md).

Other community docs: [`SECURITY.md`](SECURITY.md) · [`GOVERNANCE.md`](GOVERNANCE.md) · [`MAINTAINERS.md`](MAINTAINERS.md)

## License

rskit is distributed under the terms of the [MIT License](LICENSE).

Copyright (c) 2024 kbukum contributors.
