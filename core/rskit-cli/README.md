# rskit-cli — CLI Framework

CLI framework: progress bars, structured output, and signal handling.

[![CI](https://github.com/kbukum/rskit/actions/workflows/ci.yml/badge.svg)](https://github.com/kbukum/rskit/actions/workflows/ci.yml) [![crates.io](https://img.shields.io/crates/v/rskit-cli.svg)](https://crates.io/crates/rskit-cli) [![docs.rs](https://docs.rs/rskit-cli/badge.svg)](https://docs.rs/rskit-cli) [![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/kbukum/rskit/blob/main/LICENSE) [![MSRV: 1.91](https://img.shields.io/badge/MSRV-1.91-orange.svg)](https://www.rust-lang.org/)

## Features

- `ProgressBar` / `MultiProgress` — preset styles (Bar, Spinner, Download, Finished) over `indicatif`
- `CancellationToken` — cooperative Ctrl+C handling for async tasks
- `OutputTable` — structured table formatting for terminal output
- `OutputKV` — key-value pair formatting
- `ErrorRenderer` — render `AppError` as text, JSON, or YAML
- `ExitCode` — map `ErrorCode` values to stable process exit codes
- Steady tick, prefix/message, and position tracking

## Usage

```toml
[dependencies]
rskit-cli = "0.2.0-alpha.1"
```

```rust
use rskit_cli::{ErrorRenderer, ExitCode, OutputFormat};
use rskit_errors::{AppError, ErrorCode};

let error = AppError::new(ErrorCode::InvalidInput, "bad argument");
let (rendered, exit_code) = ErrorRenderer::new(OutputFormat::Json).render(&error);
assert!(rendered.contains("\"code\""));
assert_eq!(exit_code, ExitCode::from(error.code()));
assert_eq!(exit_code.as_i32(), 2);
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
