# rskit-pipeline — Async Stream Operators

`RskitStreamExt` adds lazy, backpressure-aware async operators to any `futures::Stream`.

[![CI](https://github.com/kbukum/rskit/actions/workflows/ci.yml/badge.svg)](https://github.com/kbukum/rskit/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/rskit-pipeline.svg)](https://crates.io/crates/rskit-pipeline)
[![docs.rs](https://docs.rs/rskit-pipeline/badge.svg)](https://docs.rs/rskit-pipeline)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![MSRV: 1.85](https://img.shields.io/badge/MSRV-1.85-orange.svg)](https://blog.rust-lang.org/2025/02/20/Rust-1.85.0.html)

## Features

Operators available on any `futures::Stream` via `RskitStreamExt`:

- `rmap` — async map
- `rflatmap` — async flat-map
- `rfilter` — predicate filter
- `rtap` — side-effect tap
- `rreduce` — async fold/reduce
- `rparallel` — concurrent execution with bounded parallelism
- `rfan_out` — broadcast to multiple async functions
- `rmerge` — merge two streams
- `rpartition` — split into matching and non-matching streams
- `rbatch` — collect into fixed-size/time-limited batches
- `rdebounce` — debounce with a quiet period
- `rthrottle` — rate-limited throughput
- `rtumbling_window` — non-overlapping time windows
- `rsliding_window` — overlapping item-count windows
- `rdistinct` — first occurrence only
- `rtake` / `rskip` — bounded prefix operators
- `rbuffer` — bounded producer/consumer decoupling

## Operator parity

Rows intentionally mirror gokit's operator order for the cross-kit Group 03 roll-up.

| Operator | rskit API | Semantics |
|----------|-----------|-----------|
| map | `RskitStreamExt::rmap` | Transform each item with a fallible async function. |
| filter | `RskitStreamExt::rfilter` | Keep items matching a synchronous predicate. |
| batch | `RskitStreamExt::rbatch` | Emit up to `size` items, or the partial batch when `timeout` elapses/upstream ends. |
| window | `RskitStreamExt::rtumbling_window` | Emit non-overlapping fixed-duration windows. |
| sliding | `RskitStreamExt::rsliding_window` | Emit overlapping item-count windows advancing by `step`. |
| fan_out | `RskitStreamExt::rfan_out` | Apply multiple async functions to each item and collect per-item results. |
| parallel | `RskitStreamExt::rparallel` | Process items concurrently with bounded parallelism; output order is not guaranteed. |
| merge | `RskitStreamExt::rmerge`, `merge` | Yield items from whichever input stream is ready first. |
| partition | `RskitStreamExt::rpartition` | Route each item to matching or remainder stream; one closed side does not close the other. |
| throttle | `RskitStreamExt::rthrottle` | Emit at most one item per interval. |
| debounce | `RskitStreamExt::rdebounce` | Wait for a quiet period, then emit the latest item. |
| distinct | `RskitStreamExt::rdistinct` | Emit the first occurrence of each item. |
| take | `RskitStreamExt::rtake` | Emit only the first `n` items. |
| skip | `RskitStreamExt::rskip` | Drop the first `n` items. |
| buffer | `RskitStreamExt::rbuffer` | Decouple producer and consumer with a bounded Tokio channel; `0` is clamped to capacity `1`. |

## Usage

```toml
[dependencies]
rskit-pipeline = "0.1"
```

```rust
use futures_util::StreamExt;
use rskit_pipeline::{from_slice, RskitStreamExt};

let results = from_slice(vec![1u32, 2, 3, 4, 5])
    .rfilter(|&n| n % 2 == 0)
    .rmap(|n| async move { Ok(n * 10) })
    .collect::<Vec<_>>()
    .await;
// [Ok(20), Ok(40)]
```

## See Also

[Main repository README](https://github.com/kbukum/rskit)
