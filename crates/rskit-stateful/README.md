# rskit-stateful — Stateful Accumulators

Stateful accumulation primitives with pluggable stores, configurable flush triggers, size limits,
TTL expiration, and keyed manager cleanup.

[![CI](https://github.com/kbukum/rskit/actions/workflows/ci.yml/badge.svg)](https://github.com/kbukum/rskit/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/rskit-stateful.svg)](https://crates.io/crates/rskit-stateful)
[![docs.rs](https://docs.rs/rskit-stateful/badge.svg)](https://docs.rs/rskit-stateful)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![MSRV: 1.85](https://img.shields.io/badge/MSRV-1.85-orange.svg)](https://blog.rust-lang.org/2025/02/20/Rust-1.85.0.html)

## Features

- `Accumulator<V>` for buffered append/flush workflows
- `AccumulatorConfig<V>` with TTL, keep-alive, max-size, triggers, and custom measurers
- `Trigger<V>` implementations for size, byte-size, and time-based flushes
- `MemoryStore<V>` plus a `Store<V>` trait for custom backends
- `Manager<K, V>` for per-key accumulators and expiration cleanup

## Usage

```toml
[dependencies]
rskit-stateful = "0.1"
```

```rust
use std::sync::Arc;
use std::time::Duration;

use rskit_stateful::{Accumulator, AccumulatorConfig, MemoryStore, SizeTrigger};

let accumulator = Accumulator::new(
    Box::new(MemoryStore::<u32>::new()),
    AccumulatorConfig::new()
        .with_max_size(1_000)
        .with_ttl(Duration::from_secs(30))
        .with_trigger(Arc::new(SizeTrigger::new(3))),
);

assert!(accumulator.append(1)?.is_none());
assert!(accumulator.append(2)?.is_none());
assert_eq!(accumulator.append(3)?, Some(vec![1, 2, 3]));
# Ok::<(), rskit_errors::AppError>(())
```

## Key types

- `Accumulator<V>` — append values, flush manually, or flush when triggers fire
- `AccumulatorConfig<V>` — builder for TTL, keep-alive, max-size, triggers, and measurer
- `Manager<K, V>` — route values to keyed accumulators and clean up expired entries
- `Store<V>` — backend abstraction for buffered values
- `MemoryStore<V>` — in-memory store implementation
- `CountMeasurer`, `ByteSizeMeasurer` — built-in size measurement strategies

## See also

[Main repository README](https://github.com/kbukum/rskit)
