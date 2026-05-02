# rskit-storage — File I/O & Storage

File I/O, local storage, temp files, MIME detection, and storage backend traits.

[![CI](https://github.com/kbukum/rskit/actions/workflows/ci.yml/badge.svg)](https://github.com/kbukum/rskit/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/rskit-storage.svg)](https://crates.io/crates/rskit-storage)
[![docs.rs](https://docs.rs/rskit-storage/badge.svg)](https://docs.rs/rskit-storage)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![MSRV: 1.85](https://img.shields.io/badge/MSRV-1.85-orange.svg)](https://blog.rust-lang.org/2025/02/20/Rust-1.85.0.html)

## Features

- `FileSource` — unified reference to local path, URL, bytes, or temp file
- `FileSink` / `FileWriter` — buffered output destinations
- `FileMeta`, `FileKind` — metadata and broad category detection (Video, Audio, Image, …)
- `detect_mime` / `detect_kind` — magic-byte + extension fallback
- `FileStore` trait with the built-in `LocalStore` and support for external adapter crates
- `TempFile` / `TempDir` — RAII-managed temporaries (auto-deleted on drop)
- Async streaming reads and file transfers

Cloud backends live in adapter crates such as `rskit-storage-s3` and
`rskit-storage-gcs` so core storage remains lightweight and reusable.

## Usage

```toml
[dependencies]
rskit-storage = "0.1"
```

```rust
use rskit_storage::{FileSource, detect_kind, FileKind, TempFile};
use rskit_errors::AppResult;

async fn example() -> AppResult<()> {
    let src = FileSource::from_path("photo.jpg");
    let kind = detect_kind(&src).await?;
    assert!(matches!(kind, FileKind::Image));

    let tmp = TempFile::new("upload", ".jpg")?;
    println!("temp path: {:?}", tmp.path());
    // tmp is deleted when dropped
    Ok(())
}
```

## See Also

[Main repository README](https://github.com/kbukum/rskit)
