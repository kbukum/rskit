# rskit-media — Media Processing Types

Media types, codec/format registry, pipeline builder, and processing traits.

[![CI](https://github.com/kbukum/rskit/actions/workflows/ci.yml/badge.svg)](https://github.com/kbukum/rskit/actions/workflows/ci.yml) [![crates.io](https://img.shields.io/crates/v/rskit-media.svg)](https://crates.io/crates/rskit-media) [![docs.rs](https://docs.rs/rskit-media/badge.svg)](https://docs.rs/rskit-media) [![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/kbukum/rskit/blob/main/LICENSE) [![MSRV: 1.91](https://img.shields.io/badge/MSRV-1.91-orange.svg)](https://github.com/kbukum/rskit/blob/main/core/Cargo.toml)

## Features

- `MediaPipeline` — lazy,
  chainable pipeline builder (extract, resize, crop, rotate, volume, speed, transcode, …)
- `MediaExecutor` / `MediaProbe` traits —
  backends implement these (see `rskit-media-ffmpeg`, `rskit-media-image`)
- Rich type vocabulary: `Codec`, `Format`, `Resolution`, `TimeRange`, `OutputConfig`, `Filter`
- `Registry` — codec/format compatibility checking
- `Progress` reporting with position, percentage, speed, and ETA
- Subtitle support: `SubtitleTrack`, `SubtitleEntry`

## Usage

```toml
[dependencies]
rskit-media = "0.2.0-alpha.2"
```

```rust
use rskit_media::{MediaPipeline, spatial::Resolution, time::TimeRange, MediaOp};
use rskit_storage::FileSource;

let source = FileSource::from_path("input.mp4");
let pipeline = MediaPipeline::from(&source)
    .extract(TimeRange::from_seconds(10.0, 60.0))
    .resize(Resolution::p1080(), Default::default());

println!("{} operations queued", pipeline.operations().len());
// Execute with a MediaExecutor backend (rskit-media-ffmpeg or rskit-media-image)
```

## See Also

[Main repository README](https://github.com/kbukum/rskit)
