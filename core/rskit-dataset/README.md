# rskit-dataset — Streaming Dataset Collection Framework

Streaming dataset collection framework: one generic collection engine drives both `DataItem` blobs
and `DatasetRecord` rows through streaming sources, transforms, per-item sinks,
and schema validation.

[![CI](https://github.com/kbukum/rskit/actions/workflows/ci.yml/badge.svg)](https://github.com/kbukum/rskit/actions/workflows/ci.yml) [![crates.io](https://img.shields.io/crates/v/rskit-dataset.svg)](https://crates.io/crates/rskit-dataset) [![docs.rs](https://docs.rs/rskit-dataset/badge.svg)](https://docs.rs/rskit-dataset) [![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/kbukum/rskit/blob/main/LICENSE) [![MSRV: 1.91](https://img.shields.io/badge/MSRV-1.91-orange.svg)](https://github.com/kbukum/rskit/blob/main/core/Cargo.toml)

## Features

- Generic collection engine: one `Collector<T>` orchestrates any `DatasetItem` —
  both `DataItem` blobs and `DatasetRecord` rows share the same worker pool, cancellation,
  and event loop
- ETL pipeline: streaming `Source` → fallible `Transform` → per-item `ItemSink<T>` materialization,
  with a pluggable `Validator<T>` policy callers opt into
- `LocalBlobSink` writes `DataItem` samples to `real/` and `ai/`;
  `DataItem` uses checked in-memory payload construction for small samples
  and file-backed streaming payloads for large samples
- Parallel fetching with configurable concurrency
- `Manifest` — incremental build cache for resumable collection
- `DatasetLimits` — configurable memory threshold and bounded stream buffers
- Schema validation delegated to `rskit-schema`
- Stream adapters for `rskit-stream`
- Streaming `DatasetRecord` readers/writers with JSON Lines
  and CSV support plus filter/column-selection operators
- Bounded JSON record size, nesting depth, field count, array length,
  and string length for untrusted records
- Progress callback support via `ProgressCallback`

## Usage

```toml
[dependencies]
rskit-dataset = "0.2.0-alpha.2"
```

```rust
use rskit_dataset::{CollectorConfig, DataItem, DatasetLimits, Label, MediaType};
use std::path::PathBuf;

let config = CollectorConfig {
    output_dir: PathBuf::from("dataset_out"),
    concurrency: 4,
    source_timeout_secs: 300.0,
    force: false,
    limits: DatasetLimits::default(),
};

let item = DataItem::new_bytes(vec![1, 2, 3], Label::Real, MediaType::Image, "local")?
    .with_extension(".bin");

// Implement Source and Transform, pick an ItemSink (e.g. LocalBlobSink), then construct Collector::new(...)
```

## See Also

[Main repository README](https://github.com/kbukum/rskit)
