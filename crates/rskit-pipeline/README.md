# rskit-pipeline — Async Stream Operators

`RskitStreamExt` extension trait adding 13 lazy async operators to any `futures::Stream`.

[![CI](https://github.com/kbukum/rskit/actions/workflows/ci.yml/badge.svg)](https://github.com/kbukum/rskit/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/rskit-pipeline.svg)](https://crates.io/crates/rskit-pipeline)
[![docs.rs](https://docs.rs/rskit-pipeline/badge.svg)](https://docs.rs/rskit-pipeline)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![MSRV: 1.85](https://img.shields.io/badge/MSRV-1.85-orange.svg)](https://blog.rust-lang.org/2025/02/20/Rust-1.85.0.html)

## Features

Operators available on any `futures::Stream` via `RskitStreamExt`:

- `rmap` — async map
- `rflatmap` — async flat-map
- `rfilter` — async filter
- `rtap` — side-effect tap
- `rreduce` — async fold/reduce
- `rparallel` — concurrent execution with bounded parallelism
- `rfan_out` — broadcast to multiple sinks
- `rbatch` — collect into fixed-size batches
- `rdebounce` — debounce with a quiet period
- `rthrottle` — rate-limited throughput
- `rtumbling_window` — non-overlapping time windows
- `rsliding_window` — overlapping time windows

## Usage

```toml
[dependencies]
rskit-pipeline = "0.1"
```

```rust
use rskit_pipeline::{RskitStreamExt, from_slice};

let results = from_slice(vec![1u32, 2, 3, 4, 5])
    .rfilter(|&n| async move { n % 2 == 0 })
    .rmap(|n| async move { Ok(n * 10) })
    .collect::<Vec<_>>()
    .await;
// [Ok(20), Ok(40)]
```

## See Also

[Main repository README](https://github.com/kbukum/rskit)
