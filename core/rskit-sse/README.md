# rskit-sse — Server-Sent Events Bus

Server-Sent Events broadcast bus with bounded replay and Axum adapters.

[![CI](https://github.com/kbukum/rskit/actions/workflows/ci.yml/badge.svg)](https://github.com/kbukum/rskit/actions/workflows/ci.yml) [![crates.io](https://img.shields.io/crates/v/rskit-sse.svg)](https://crates.io/crates/rskit-sse) [![docs.rs](https://docs.rs/rskit-sse/badge.svg)](https://docs.rs/rskit-sse) [![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE) [![MSRV: 1.91](https://img.shields.io/badge/MSRV-1.91-orange.svg)](https://github.com/kbukum/rskit/blob/main/core/Cargo.toml)

## Features

- `SseBus<T>` — bounded broadcast-backed event bus for any `Clone + Serialize` type
- `publish(event)` assigns an event id, stores bounded replay, and sends to active subscribers
- `subscribe()` returns toolkit-native `SseEvent<T>` values
- `subscribe_after(last_event_id)` replays retained events newer than a client cursor
- `subscribe_axum()` and `subscribe_axum_after()` adapt events to Axum SSE streams
- `subscriber_count()` for monitoring

## Delivery contract

- Live subscribers receive events published after they subscribe.
- Reconnecting subscribers can replay retained events with `Last-Event-ID`.
- Per-subscriber ordering follows publish order for delivered events.
- Slow subscribers may skip lagged live events; skipped events can be recovered only while they remain in the bounded replay buffer.

## Usage

```toml
[dependencies]
rskit-sse = "0.1.0-alpha.1"
```

```rust
use rskit_sse::SseBus;
use serde::Serialize;

#[derive(Clone, Serialize)]
struct Status { progress: u32 }

let bus = SseBus::new(16)?;

// In an Axum handler: return bus.subscribe_axum() as an SSE response stream.
bus.publish(Status { progress: 50 }).unwrap();
println!("subscribers: {}", bus.subscriber_count());
```

## See Also

[Main repository README](https://github.com/kbukum/rskit)
