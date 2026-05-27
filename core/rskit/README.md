# rskit — Production Rust Toolkit

Facade crate that re-exports all `rskit-*` sub-crates. Single dependency to get the full toolkit.

[![CI](https://github.com/kbukum/rskit/actions/workflows/ci.yml/badge.svg)](https://github.com/kbukum/rskit/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/rskit.svg)](https://crates.io/crates/rskit)
[![docs.rs](https://docs.rs/rskit/badge.svg)](https://docs.rs/rskit)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![MSRV: 1.85](https://img.shields.io/badge/MSRV-1.85-orange.svg)](https://blog.rust-lang.org/2025/02/20/Rust-1.85.0.html)

## Features

- **errors** — structured `AppError` + `ErrorCode` + RFC 9457 problem details + HTTP status metadata
- **config** — layered TOML / `.env` / env-var configuration loading
- **logging** — one-call `tracing` subscriber setup (JSON or console)
- **resilience** — retry, circuit breaker, bulkhead, rate limiter + Tower layers
- **provider** — async interaction traits with Tower bridge
- **pipeline** — lazy async stream operators (`RskitStreamExt`)
- **worker** — bounded async worker pool with typed event streaming
- **server** *(optional)* — `tonic` gRPC server as a lifecycle component

## Feature Flags

| Feature  | Included                      |
|----------|-------------------------------|
| `server` | `rskit-server` (tonic gRPC)   |
| `full`   | all features                  |

## Usage

```toml
[dependencies]
rskit = "0.1"
# or with gRPC server support:
# rskit = { version = "0.1", features = ["full"] }
```

```rust
use rskit::prelude::*;

let err = AppError::not_found("user", "42");
println!("{}", err); // NotFound: user '42' not found
```

## See Also

[Main repository README](https://github.com/kbukum/rskit)
