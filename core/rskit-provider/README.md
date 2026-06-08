# rskit-provider — Async I/O Interaction Traits

Typed async interaction traits (`Provider`, `RequestResponse`, `Stream`, `Sink`, `Duplex`) with a Tower bridge.

[![CI](https://github.com/kbukum/rskit/actions/workflows/ci.yml/badge.svg)](https://github.com/kbukum/rskit/actions/workflows/ci.yml) [![crates.io](https://img.shields.io/crates/v/rskit-provider.svg)](https://crates.io/crates/rskit-provider) [![docs.rs](https://docs.rs/rskit-provider/badge.svg)](https://docs.rs/rskit-provider) [![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/kbukum/rskit/blob/main/LICENSE) [![MSRV: 1.91](https://img.shields.io/badge/MSRV-1.91-orange.svg)](https://github.com/kbukum/rskit/blob/main/core/Cargo.toml)

## Features

- `RequestResponse<I,O>` — single request → single response
- `Stream<I,O>` — request → async stream of responses
- `Sink<I>` — fire-and-forget
- `Duplex<I,O>` — bidirectional streaming
- Tower bridge for adapting `tower::Service` to `RequestResponse`

The provider traits intentionally remain object-safe via `async-trait` because some downstream adapters store them behind `dyn` trait objects. Stream, sink, and duplex implementations must use bounded queues or downstream backpressure and surface saturation as typed `AppError` values.

Cross-cutting behavior such as retry, rate limiting, circuit breaking, logging, and tracing belongs above these L2 contracts. Compose those concerns with `rskit-resilience` tower layers before wrapping a service with `TowerProvider`.

## Usage

```toml
[dependencies]
rskit-provider = "0.1.0-alpha.1"
async-trait = "0.1"
```

```rust
use rskit_provider::{Provider, RequestResponse};

struct EchoProvider;

#[async_trait::async_trait]
impl Provider for EchoProvider {
    fn name(&self) -> &'static str {
        "echo"
    }
}

#[async_trait::async_trait]
impl RequestResponse<String, String> for EchoProvider {
    async fn execute(&self, input: String) -> rskit_errors::AppResult<String> {
        Ok(input.to_uppercase())
    }
}
```

## See Also

[Main repository README](https://github.com/kbukum/rskit)
