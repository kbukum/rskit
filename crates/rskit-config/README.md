# rskit-config — Layered Config Loading

Load application config from TOML files, `.env` files, and environment variables with a clear priority order.

[![CI](https://github.com/kbukum/rskit/actions/workflows/ci.yml/badge.svg)](https://github.com/kbukum/rskit/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/rskit-config.svg)](https://crates.io/crates/rskit-config)
[![docs.rs](https://docs.rs/rskit-config/badge.svg)](https://docs.rs/rskit-config)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![MSRV: 1.85](https://img.shields.io/badge/MSRV-1.85-orange.svg)](https://blog.rust-lang.org/2025/02/20/Rust-1.85.0.html)

## Features

- Layered loading: TOML file → `.env` → `APP__SECTION__KEY` env vars
- Serde-based deserialization into any `Deserialize` + `Validate` type
- Optional `.env` file support via `dotenvy`
- Configurable env prefix

## Loading Order (lowest → highest priority)

1. TOML file (`with_config_file`)
2. `.env` file
3. `APP__SECTION__KEY` environment variables

## Usage

```toml
[dependencies]
rskit-config = "0.1"
serde = { version = "1", features = ["derive"] }
validator = { version = "0.18", features = ["derive"] }
```

```rust
use rskit_config::{ConfigLoader, AppConfig};
use serde::Deserialize;
use validator::Validate;

#[derive(Deserialize, Validate)]
struct Config {
    #[validate(length(min = 1))]
    name: String,
    port: u16,
}

let cfg: Config = ConfigLoader::new()
    .with_config_file("config/app.toml")
    .with_env_prefix("MYAPP")
    .load()?;
```

## See Also

[Main repository README](https://github.com/kbukum/rskit)
