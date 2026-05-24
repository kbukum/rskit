# rskit-media-image — Native Image Backend

Native image processing backend using the `image` crate.

[![CI](https://github.com/kbukum/rskit/actions/workflows/ci.yml/badge.svg)](https://github.com/kbukum/rskit/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/rskit-media-image.svg)](https://crates.io/crates/rskit-media-image)
[![docs.rs](https://docs.rs/rskit-media-image/badge.svg)](https://docs.rs/rskit-media-image)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![MSRV: 1.85](https://img.shields.io/badge/MSRV-1.85-orange.svg)](https://blog.rust-lang.org/2025/02/20/Rust-1.85.0.html)

## Features

- `register` — installs native image executor and probe factories into `rskit-media`
- Spatial ops: resize (Fit / Fill / Pad), crop, rotate, flip
- Filters: blur, sharpen, brightness, saturation, hue shift
- Format support: PNG, JPEG, GIF, BMP, TIFF, WebP, AVIF
- No FFmpeg subprocess required — fast native processing

## Usage

```toml
[dependencies]
rskit-media-image = "0.1"
```

```rust
use rskit_media::{MediaPipeline, Registry, spatial::Resolution};
use rskit_media_image::{Config, register};
use rskit_storage::FileSource;

async fn example() -> rskit_errors::AppResult<()> {
    let mut registry = Registry::default();
    register(&mut registry, Config)?;
    let processor = registry.executor("image")?;
    let source = FileSource::from_path("photo.jpg");

    let pipeline = MediaPipeline::from(&source)
        .resize(Resolution::new(800, 600), Default::default());

    // Execute using the native image processor
    // let result = processor.execute(&pipeline, &sink, progress_cb).await?;
    Ok(())
}
```

## See Also

[Main repository README](https://github.com/kbukum/rskit)
