# rskit-provider — Async I/O Interaction Traits

Typed async interaction traits (`Provider`, `RequestResponse`, `Sink`, `Duplex`) with Tower bridge and middleware layers.

[![CI](https://github.com/kbukum/rskit/actions/workflows/ci.yml/badge.svg)](https://github.com/kbukum/rskit/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/rskit-provider.svg)](https://crates.io/crates/rskit-provider)
[![docs.rs](https://docs.rs/rskit-provider/badge.svg)](https://docs.rs/rskit-provider)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![MSRV: 1.85](https://img.shields.io/badge/MSRV-1.85-orange.svg)](https://blog.rust-lang.org/2025/02/20/Rust-1.85.0.html)

## Features

- `RequestResponse<I,O>` — single request → single response
- `StreamProvider<I,O>` — request → async stream of responses
- `Sink<I>` — fire-and-forget
- `Duplex<I,O>` — bidirectional streaming
- Tower bridge for composing providers with middleware

## Usage

```toml
[dependencies]
rskit-provider = "0.1"
async-trait = "0.1"
```

```rust
use rskit_provider::traits::RequestResponse;

struct EchoProvider;

#[async_trait::async_trait]
impl RequestResponse<String, String> for EchoProvider {
    async fn call(&self, input: String) -> rskit_errors::AppResult<String> {
        Ok(input.to_uppercase())
    }
}
```

## See Also

[Main repository README](https://github.com/kbukum/rskit)
