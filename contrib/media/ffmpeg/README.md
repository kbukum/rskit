# rskit-media-ffmpeg — FFmpeg Backend

FFmpeg CLI backend for video/audio processing.

[![CI](https://github.com/kbukum/rskit/actions/workflows/ci.yml/badge.svg)](https://github.com/kbukum/rskit/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/rskit-media-ffmpeg.svg)](https://crates.io/crates/rskit-media-ffmpeg)
[![docs.rs](https://docs.rs/rskit-media-ffmpeg/badge.svg)](https://docs.rs/rskit-media-ffmpeg)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![MSRV: 1.85](https://img.shields.io/badge/MSRV-1.85-orange.svg)](https://blog.rust-lang.org/2025/02/20/Rust-1.85.0.html)

## Features

- `register` — installs FFmpeg executor and probe factories into `rskit-media`
- Hardware acceleration support (`HwAccel`: CUDA, VAAPI, etc.)
- Real-time progress parsing from FFmpeg output
- Configurable log levels via `FfmpegLogLevel`
- Compiles `MediaPipeline` operations into FFmpeg CLI arguments

## Usage

```toml
[dependencies]
rskit-media-ffmpeg = "0.1"
```

```rust
use rskit_media::registry::Registry;
use rskit_media_ffmpeg::{Config, register};

async fn example() {
    let mut registry = Registry::default();
    register(&mut registry, Config::default())?;
    let executor = registry.executor("ffmpeg")?;

    // Probe a file for metadata
    let probe = registry.probe("ffmpeg")?;
    // let meta = probe.probe(&FileSource::from_path("video.mp4")).await?;

    // Execute a MediaPipeline via FFmpeg subprocess
    // let result = executor.execute(&pipeline, &sink, progress_cb).await?;
}
```

## See Also

[Main repository README](https://github.com/kbukum/rskit)
