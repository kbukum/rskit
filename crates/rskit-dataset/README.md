# rskit-dataset — Dataset Collection Framework

Dataset collection framework: source, transform, target, and collector orchestrator.

[![CI](https://github.com/kbukum/rskit/actions/workflows/ci.yml/badge.svg)](https://github.com/kbukum/rskit/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/rskit-dataset.svg)](https://crates.io/crates/rskit-dataset)
[![docs.rs](https://docs.rs/rskit-dataset/badge.svg)](https://docs.rs/rskit-dataset)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![MSRV: 1.85](https://img.shields.io/badge/MSRV-1.85-orange.svg)](https://blog.rust-lang.org/2025/02/20/Rust-1.85.0.html)

## Features

- ETL pipeline: `Source` → `Transform` → `Target` orchestrated by `Collector`
- `DataItem` with content bytes, `Label` (Real / AiGenerated), and `MediaType` (Image, Text, Audio, Video)
- Parallel fetching with configurable concurrency
- `Manifest` — incremental build cache for resumable collection
- `CollectorConfig` — output dir, concurrency, timeout, force-rebuild
- Progress callback support via `ProgressCallback`

## Usage

```toml
[dependencies]
rskit-dataset = "0.1"
```

```rust
use rskit_dataset::{Collector, CollectorConfig, DataItem, Label, MediaType};
use std::path::PathBuf;

let config = CollectorConfig {
    output_dir: PathBuf::from("dataset_out"),
    concurrency: 4,
    source_timeout_secs: 300.0,
    force: false,
};
let collector = Collector::new(config).unwrap();

// Implement Source, Transform, Target traits for your pipeline
// let result = collector.collect(sources, transforms, targets).await?;
```

## See Also

[Main repository README](https://github.com/kbukum/rskit)
