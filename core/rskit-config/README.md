# rskit-config — Adapter-Oriented Config Loading

Load application, service, tool, and adapter-backed configuration through one source pipeline.

[![CI](https://github.com/kbukum/rskit/actions/workflows/ci.yml/badge.svg)](https://github.com/kbukum/rskit/actions/workflows/ci.yml) [![crates.io](https://img.shields.io/crates/v/rskit-config.svg)](https://crates.io/crates/rskit-config) [![docs.rs](https://docs.rs/rskit-config/badge.svg)](https://docs.rs/rskit-config) [![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/kbukum/rskit/blob/main/LICENSE) [![MSRV: 1.91](https://img.shields.io/badge/MSRV-1.91-orange.svg)](https://github.com/kbukum/rskit/blob/main/core/Cargo.toml)

## Features

- Layered app loading: defaults → TOML file → `.env` → adapter sources → `APP__SECTION__KEY` env vars → explicit overrides
- Serde-based deserialization into any `Deserialize` + `Validate` type
- Optional `.env` file support via `dotenvy`
- Configurable env prefix
- Programmatic defaults and overrides with deterministic precedence
- Profile-specific env files (`config/profiles/{profile}.env`)
- Secret config fields via `SecretString`, which redacts `Debug`/`Display` output
- Adapter contract for external config sources such as Vault, SSM Parameter Store, Kubernetes secrets, or remote config services

## Loading Order (lowest → highest priority)

1. Programmatic defaults (`with_default`)
2. TOML file (`with_config_file`)
3. Profile env file (`config/profiles/{profile}.env`, via `with_profile`)
4. `.env` file
5. Adapter sources (`with_source`)
6. Environment variables (`__` separator, optional prefix via `with_env_prefix`)
7. Programmatic overrides (`with_override`)

Profile files requested through `with_profile` and explicit `.env` files requested through `with_env_file` are fail-closed: missing or malformed files return an `AppError` during `load()` instead of being silently ignored.

Dotenv values are loaded into the `ConfigLoader` source chain only; they do not mutate the process environment. Code that needs dotenv-backed values should read them from the typed config returned by `load()` rather than from `std::env`. Malformed auto-discovered `.env` files are logged and skipped so optional local developer files do not prevent startup.

## Usage

```toml
[dependencies]
rskit-config = "0.1.0-alpha.1"
serde = { version = "1", features = ["derive"] }
validator = { version = "0.20", features = ["derive"] }
```

```rust
use rskit_config::{AppConfig, ConfigLoader, ConfigSource, SecretString, ServiceConfig};
use serde::Deserialize;
use validator::Validate;

#[derive(Deserialize, Validate)]
struct Config {
    #[serde(flatten)]
    #[validate(nested)]
    service: ServiceConfig,
    #[validate(range(min = 1))]
    grpc_port: u16,
    api_token: SecretString,
}

impl AppConfig for Config {
    fn apply_defaults(&mut self) {}
    fn service_config(&self) -> &ServiceConfig { &self.service }
}

let cfg: Config = ConfigLoader::app()
    .with_default("grpc_port", 50051_i64)
    .with_config_file("config/app.toml")
    .with_env_prefix("MYAPP")
    .with_override("name", "api")
    .load_app()?;
```

Use [`SecretString`](https://docs.rs/rskit-config/latest/rskit_config/struct.SecretString.html) for credentials, tokens, private keys, and other secret fields. It deserializes from config sources but masks `Debug`, `Display`, and serialization output; call `expose()` only at the boundary that needs the plaintext.

Deterministic tool/project config should use an explicit policy that does not read dotenv files or process environment variables:

```rust
let cfg: ToolConfig = ConfigLoader::toml("tool.toml").load()?;
```

External backends plug in through the adapter contract:

```rust
#[derive(Debug)]
struct VaultSource;

impl ConfigSource for VaultSource {
    fn collect(&self) -> rskit_errors::AppResult<config::Config> {
        // Fetch and convert backend values into a config::Config.
        config::Config::builder()
            .build()
            .map_err(|e| rskit_errors::AppError::invalid_input("config", e.to_string()))
    }
}

let cfg: Config = ConfigLoader::app()
    .with_source(VaultSource)
    .load_app()?;
```

## ServiceConfig Fields

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `name` | `String` | `"service"` | Service name |
| `environment` | `Environment` | `Development` | Deployment environment |
| `version` | `String` | package version | Service version |
| `address` | `String` | `"0.0.0.0"` | Service bind address |
| `port` | `u16` | `50051` | Service port |
| `debug` | `bool` | `false` | Debug mode |
| `logging` | `LoggingConfig` | | Logging (level, format, output) |

## Validation

Structs loaded with `load()` must implement `Deserialize` and `Validate`. Application configs loaded with `load_app()` must implement `AppConfig`; the loader calls `AppConfig::apply_defaults()` before validation.

## See Also

[Main repository README](https://github.com/kbukum/rskit)
