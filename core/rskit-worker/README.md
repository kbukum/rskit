# rskit-worker — Bounded Async Worker Pool

`JoinSet` + `Semaphore` worker pool with typed event streaming, panic detection, and graceful drain.

[![CI](https://github.com/kbukum/rskit/actions/workflows/ci.yml/badge.svg)](https://github.com/kbukum/rskit/actions/workflows/ci.yml) [![crates.io](https://img.shields.io/crates/v/rskit-worker.svg)](https://crates.io/crates/rskit-worker) [![docs.rs](https://docs.rs/rskit-worker/badge.svg)](https://docs.rs/rskit-worker) [![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/kbukum/rskit/blob/main/LICENSE) [![MSRV: 1.91](https://img.shields.io/badge/MSRV-1.91-orange.svg)](https://github.com/kbukum/rskit/blob/main/core/Cargo.toml)

## Features

- `Handler<I,O>` trait for implementing task logic
- `Event<O>` with progress / partial / log / result / error kinds
- `TaskHandle` for result retrieval, event subscription, and cancellation
- mpsc → broadcast relay (no `T: Sync` required)
- Panic detection and graceful pool drain

## Usage

```toml
[dependencies]
rskit-worker = "0.2.0-alpha.2"
async-trait = "0.1"
```

```rust
use rskit_worker::{Handler, Pool, PoolConfig, Event};

struct UpperHandler;

#[async_trait::async_trait]
impl Handler<String, String> for UpperHandler {
    async fn handle(&self, input: String,
        _emit: tokio::sync::mpsc::Sender<Event<String>>,
        _cancel: tokio_util::sync::CancellationToken,
    ) -> rskit_errors::AppResult<String> {
        Ok(input.to_uppercase())
    }
}

let pool = Pool::new(Arc::new(UpperHandler), PoolConfig::new("demo"));
let handle = pool.submit("hello".to_string()).await?;
assert_eq!(handle.result().await?, "HELLO");
```

## See Also

[Main repository README](https://github.com/kbukum/rskit)
