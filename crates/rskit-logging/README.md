# rskit-logging — Tracing Subscriber Setup

One-call setup for `tracing` subscribers with JSON or console format, env-filter support, and a drop-based guard.

[![CI](https://github.com/kbukum/rskit/actions/workflows/ci.yml/badge.svg)](https://github.com/kbukum/rskit/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/rskit-logging.svg)](https://crates.io/crates/rskit-logging)
[![docs.rs](https://docs.rs/rskit-logging/badge.svg)](https://docs.rs/rskit-logging)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![MSRV: 1.85](https://img.shields.io/badge/MSRV-1.85-orange.svg)](https://blog.rust-lang.org/2025/02/20/Rust-1.85.0.html)

## Features

- JSON format for production, pretty console format for development
- `RUST_LOG` / `RUST_LOG_STYLE` env-filter support
- Drop-based guard ensures all buffered logs are flushed on shutdown
- Integrates with `rskit-config` for configuration-driven setup

## Usage

```toml
[dependencies]
rskit-logging = "0.1"
```

```rust
use rskit_logging::{init_logging, LoggingConfig, LogFormat};

let cfg = LoggingConfig {
    level: "info".into(),
    format: LogFormat::Json,
    ..Default::default()
};
let _guard = init_logging(&cfg)?;
tracing::info!("service started");
```

## See Also

[Main repository README](https://github.com/kbukum/rskit)
