# rskit-config — Layered Config Loading

Load application config from TOML files, `.env` files, and environment variables with a clear priority order.

[![CI](https://github.com/kbukum/rskit/actions/workflows/ci.yml/badge.svg)](https://github.com/kbukum/rskit/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/rskit-config.svg)](https://crates.io/crates/rskit-config)
[![docs.rs](https://docs.rs/rskit-config/badge.svg)](https://docs.rs/rskit-config)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![MSRV: 1.85](https://img.shields.io/badge/MSRV-1.85-orange.svg)](https://blog.rust-lang.org/2025/02/20/Rust-1.85.0.html)

## Features

- Layered loading: defaults → TOML file → `.env` → `APP__SECTION__KEY` env vars → explicit overrides
- Serde-based deserialization into any `Deserialize` + `Validate` type
- Optional `.env` file support via `dotenvy`
- Configurable env prefix
- Programmatic defaults and overrides with deterministic precedence
- Profile-specific env files (`config/profiles/{profile}.env`)

## Loading Order (lowest → highest priority)

1. Programmatic defaults (`with_default`)
2. TOML file (`with_config_file`)
3. Profile env file (`config/profiles/{profile}.env`, via `with_profile`)
4. `.env` file
5. Environment variables (`__` separator, optional prefix via `with_env_prefix`)
6. Programmatic overrides (`with_override`)

Profile files requested through `with_profile` and explicit `.env` files requested
through `with_env_file` are fail-closed: missing or malformed files return an
`AppError` during `load()` instead of being silently ignored.

Dotenv values are loaded into the `ConfigLoader` source chain only; they do not
mutate the process environment. Code that needs dotenv-backed values should read
them from the typed config returned by `load()` rather than from `std::env`.
Malformed auto-discovered `.env` files are logged and skipped so optional local
developer files do not prevent startup.

## Usage

```toml
[dependencies]
rskit-config = "0.1"
serde = { version = "1", features = ["derive"] }
validator = { version = "0.18", features = ["derive"] }
```

```rust
use rskit_config::{ConfigLoader, AppConfig, ServiceConfig};
use serde::Deserialize;
use validator::Validate;

#[derive(Deserialize, Validate)]
struct Config {
    #[serde(flatten)]
    service: ServiceConfig,
    #[validate(range(min = 1))]
    grpc_port: u16,
}

impl AppConfig for Config {
    fn apply_defaults(&mut self) {}
    fn service_config(&self) -> &ServiceConfig { &self.service }
}

let cfg: Config = ConfigLoader::new()
    .with_default("grpc_port", 50051_i64)
    .with_config_file("config/app.toml")
    .with_env_prefix("MYAPP")
    .with_override("name", "api")
    .load()?;
```

## ServiceConfig Fields

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `name` | `String` | `"service"` | Service name |
| `environment` | `Environment` | `Development` | Deployment environment |
| `version` | `String` | `rskit-version` | Service version |
| `address` | `String` | `"0.0.0.0"` | Service bind address |
| `port` | `u16` | `50051` | Service port |
| `debug` | `bool` | `false` | Debug mode |
| `logging` | `LoggingConfig` | | Logging (level, format, output) |

## Validation

Structs must implement `validator::Validate`. The loader calls `validate()` after `apply_defaults()` and returns `AppError` on failure.

## See Also

[Main repository README](https://github.com/kbukum/rskit)
