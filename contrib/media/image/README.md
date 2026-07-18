# rskit-media-image — Native Image Backend

Native image processing backend using the `image` crate.

[![CI](https://github.com/kbukum/rskit/actions/workflows/ci.yml/badge.svg)](https://github.com/kbukum/rskit/actions/workflows/ci.yml) [![crates.io](https://img.shields.io/crates/v/rskit-media-image.svg)](https://crates.io/crates/rskit-media-image) [![docs.rs](https://docs.rs/rskit-media-image/badge.svg)](https://docs.rs/rskit-media-image) [![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/kbukum/rskit/blob/main/LICENSE) [![MSRV: 1.91](https://img.shields.io/badge/MSRV-1.91-orange.svg)](https://github.com/kbukum/rskit/blob/main/contrib/Cargo.toml)

## Features

- `register` — installs native image executor and probe factories into `rskit-media`
- Spatial ops: resize (Fit / Fill / Pad), crop, rotate, flip
- Filters: blur, sharpen, brightness, saturation, hue shift
- Format support: PNG, JPEG, GIF, BMP, TIFF, WebP, AVIF
- Configurable source-byte, decoded-pixel, and decode-ratio limits for untrusted images
- No FFmpeg subprocess required — fast native processing

## Usage

```toml
[dependencies]
rskit-media-image = "0.2.0-alpha.2"
```

```rust
use rskit_media::{MediaPipeline, Registry, spatial::Resolution};
use rskit_media_image::{Config, register};
use rskit_storage::FileSource;

async fn example() -> rskit_errors::AppResult<()> {
    let mut registry = Registry::default();
    let config = Config::default()
        .with_max_source_bytes(64 * 1024 * 1024)
        .with_max_pixels(100_000_000);
    register(&mut registry, config)?;
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
