# rskit-cli — CLI Framework

CLI framework: progress bars, structured output, and signal handling.

[![CI](https://github.com/kbukum/rskit/actions/workflows/ci.yml/badge.svg)](https://github.com/kbukum/rskit/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/rskit-cli.svg)](https://crates.io/crates/rskit-cli)
[![docs.rs](https://docs.rs/rskit-cli/badge.svg)](https://docs.rs/rskit-cli)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![MSRV: 1.85](https://img.shields.io/badge/MSRV-1.85-orange.svg)](https://blog.rust-lang.org/2025/02/20/Rust-1.85.0.html)

## Features

- `ProgressBar` / `MultiProgress` — preset styles (Bar, Spinner, Download, Finished) over `indicatif`
- `CancellationToken` — cooperative Ctrl+C handling for async tasks
- `OutputTable` — structured table formatting for terminal output
- `OutputKV` — key-value pair formatting
- Steady tick, prefix/message, and position tracking

## Usage

```toml
[dependencies]
rskit-cli = "0.1"
```

```rust
use rskit_cli::{ProgressBar, ProgressStyle, CancellationToken};
use std::time::Duration;

async fn example() {
    let bar = ProgressBar::new(100, ProgressStyle::Bar);
    bar.set_prefix("Downloading");
    for i in 0..=100 {
        bar.set_position(i);
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    bar.finish_with_message("Done!");

    let token = CancellationToken::new();
    // Clone and pass to spawned tasks for cooperative shutdown
    let _child = token.clone();
}
```

## See Also

[Main repository README](https://github.com/kbukum/rskit)
