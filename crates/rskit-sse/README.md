# rskit-sse — Server-Sent Events Bus

Server-Sent Events broadcast bus with Axum integration.

[![CI](https://github.com/kbukum/rskit/actions/workflows/ci.yml/badge.svg)](https://github.com/kbukum/rskit/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/rskit-sse.svg)](https://crates.io/crates/rskit-sse)
[![docs.rs](https://docs.rs/rskit-sse/badge.svg)](https://docs.rs/rskit-sse)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![MSRV: 1.85](https://img.shields.io/badge/MSRV-1.85-orange.svg)](https://blog.rust-lang.org/2025/02/20/Rust-1.85.0.html)

## Features

- `SseBus<T>` — broadcast-backed event bus for any `Clone + Serialize` type
- `publish(event)` sends to all active subscribers
- `subscribe()` returns an Axum-compatible SSE `Stream`
- Automatic JSON serialization of events
- `subscriber_count()` for monitoring

## Usage

```toml
[dependencies]
rskit-sse = "0.1"
```

```rust
use rskit_sse::SseBus;
use serde::Serialize;

#[derive(Clone, Serialize)]
struct Status { progress: u32 }

let bus = SseBus::new(16);

// In an Axum handler: return bus.subscribe() as SSE response
bus.publish(Status { progress: 50 }).unwrap();
println!("subscribers: {}", bus.subscriber_count());
```

## See Also

[Main repository README](https://github.com/kbukum/rskit)
