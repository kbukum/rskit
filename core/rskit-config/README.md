# rskit-config — General-Purpose Config Loading

One config crate for every consumer class: services, apps, CLIs, tools, libraries, and schema-strict consumers — with a pluggable provider model (read / write / watch).

[![CI](https://github.com/kbukum/rskit/actions/workflows/ci.yml/badge.svg)](https://github.com/kbukum/rskit/actions/workflows/ci.yml) [![crates.io](https://img.shields.io/crates/v/rskit-config.svg)](https://crates.io/crates/rskit-config) [![docs.rs](https://docs.rs/rskit-config/badge.svg)](https://docs.rs/rskit-config) [![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/kbukum/rskit/blob/main/LICENSE) [![MSRV: 1.91](https://img.shields.io/badge/MSRV-1.91-orange.svg)](https://github.com/kbukum/rskit/blob/main/core/Cargo.toml)

## Capability layers

Each layer is opt-in, so a consumer links only what it needs:

| Layer | Module | For |
|-------|--------|-----|
| Source pipeline (read) | `source` | Layered load: defaults → TOML → dotenv → adapters → env → overrides |
| Typed decode | `typed` (via `ConfigLoader::load`) | Plain `Deserialize` types; validation optional |
| Service shape | `service` + `app` | `ServiceConfig` / `AppConfig` convenience for network services |
| Strict TOML | `strict` | `deny_unknown_fields`, raw-subtree passthrough, identity-aware include-merge |
| Write (sink) | `sink` | Persist/patch config back to a backend (`ConfigSink`) |
| Watch (reload) | `watch` | Dynamic-reload change stream (`ConfigWatch`, feature-gated) |

## Features

| Feature | Default | Enables |
|---------|---------|---------|
| `validate` | on | `ServiceConfig`, `AppConfig`, `load_validated*`, `load_app`, `load_config` (pulls `validator`) |
| `watch` | off | `ConfigWatch` dynamic-reload contract + in-memory watch source (pulls `rskit-stream` for the bounded fan-out `Broadcaster`, `tokio-util` for cancellation) |

Apps, CLIs, tools, and libraries that load plain serde types should set `default-features = false` to drop the `validator` dependency.

## Consumer stories

### 1. Service — `ServiceConfig` / `AppConfig`

Services want layered loading plus service identity (name/address/port/logging) and validation. Requires the default `validate` feature.

```rust
use rskit_config::{AppConfig, ConfigLoader, SecretString, ServiceConfig};
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
    .load_app()?;
```

### 2. App / CLI / tool / library — plain typed load, no service identity

No service shape, no `validator`. Set `default-features = false`. `ConfigLoader::toml(path)` is the deterministic entry point (file only — no dotenv, no process env); `ConfigLoader::custom()` reads only explicitly added sources.

```rust
use rskit_config::ConfigLoader;
use serde::Deserialize;

#[derive(Deserialize)]
struct ToolConfig {
    workers: usize,
    output_dir: String,
}

// File-only, deterministic: ideal for CLIs and one-shot tools.
let cfg: ToolConfig = ConfigLoader::toml("tool.toml").load()?;
```

Opt into validation per-call without making it the default by implementing `Validate` and calling `load_validated()` instead of `load()`.

### 3. Strict — `deny_unknown_fields` + raw passthrough + include-merge

For plugin hosts and multi-file schemas (e.g. Toven's `Document`) that must reject unknown keys, keep verbatim sub-trees for downstream owner-specific parsing, and merge includes by identity. Parses through the `toml` crate so serde's unknown-field rejection actually fires. Independent of the `validate` feature.

```rust
use std::collections::BTreeMap;
use rskit_config::{RawValue, StrictLoader, deserialize_subtree};
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Manifest {
    name: String,
    // Verbatim per-plugin sub-trees, parsed later by each owner.
    #[serde(default)]
    plugins: BTreeMap<String, RawValue>,
}

let manifest: Manifest = StrictLoader::new("manifest.toml")
    .with_include("manifest.local.toml")
    .load()?;

// Each owner parses its own sub-tree, with its own `deny_unknown_fields`.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct PluginCfg { enabled: bool }

for (name, raw) in manifest.plugins {
    let cfg: PluginCfg = deserialize_subtree(&name, raw)?;
    let _ = (name, cfg.enabled);
}
```

Identity-aware include-merge hard-errors on duplicate identities instead of silently overriding:

```rust
use rskit_config::{CompositeKey, IdentityKey, IncludeMerge};

let merge = IncludeMerge::new()
    // Arrays-of-tables keyed by a single `name` field: a duplicate `name`
    // across merged documents is a hard error, not a silent last-wins override.
    .with_identity("groups", IdentityKey::new("name"))
    // Arrays-of-tables with no single identifying field: identity is the
    // joined value of several (optionally nested) fields.
    .with_identity(
        "edges",
        CompositeKey::new(["from.module", "to.module"]),
    )
    // Tables-of-tables keyed by name (`[domains.<name>]`): a key contributed by
    // two documents is a duplicate identity, rejected rather than merged.
    .with_unique_keys("domains");
```

### 4. Pluggable providers — read / write / watch

Add a custom backend without changing core. rskit-config owns ordering/merge/decoding; backends implement the contracts and are injected explicitly. A future `rskit-vault` (or your own adapter) is a pure consumer of these traits.

**Read** — `ConfigSource` slots into the ordered pipeline:

```rust
use rskit_config::{ConfigLoader, ConfigSource};

#[derive(Debug)]
struct VaultSource;

impl ConfigSource for VaultSource {
    fn collect(&self) -> rskit_errors::AppResult<config::Config> {
        // Fetch backend values and convert them into a `config::Config`.
        config::Config::builder()
            .build()
            .map_err(|e| rskit_errors::AppError::invalid_input("config", e.to_string()))
    }
}

let loader = ConfigLoader::custom().with_source(VaultSource);
```

**Write** — `ConfigSink` persists/patches values back. Values flow as `SecretString` and are never logged. `InMemoryConfigSink` and `FileConfigSink` ship as reference impls; `FileConfigSink` reads and writes through a pluggable `rskit-codec` `Codec` (TOML by default, any format via `with_codec`):

```rust
use rskit_config::{ConfigSink, FileConfigSink, SecretString};

let sink = FileConfigSink::new("config/secrets.toml");
sink.set("api_token", SecretString::new("s3cret"))?;
sink.remove("stale_key")?;
```

**Watch** — `ConfigWatch` yields a bounded change stream (the `rskit-stream` `Broadcaster` specialized to typed change events) so a long-running consumer re-runs the pipeline and re-decodes. Behind the `watch` feature; cancellation via `CancellationToken`:

```rust,ignore
use futures::StreamExt;
use rskit_config::{ConfigChange, ConfigSink, ConfigWatch, InMemoryConfigSink, SecretString};
use tokio_util::sync::CancellationToken;

let sink = InMemoryConfigSink::new();
let cancel = CancellationToken::new();
let mut changes = sink.watch(cancel.clone())?;

sink.set("port", SecretString::new("8081"))?;
if let Some(ConfigChange::Set { key }) = changes.next().await {
    // Re-run the load pipeline and re-decode for `key`.
    let _ = key;
}
cancel.cancel();
```

## Loading order (lowest → highest priority)

1. Programmatic defaults (`with_default`)
2. TOML file (`with_config_file`)
3. Profile env file (`config/profiles/{profile}.env`, via `with_profile`)
4. `.env` file
5. Adapter sources (`with_source`)
6. Environment variables (`__` separator, optional prefix via `with_env_prefix`)
7. Programmatic overrides (`with_override`)

Profile files requested through `with_profile` and explicit `.env` files requested through `with_env_file` are fail-closed: missing or malformed files return an `AppError` during `load()` instead of being silently ignored.

Dotenv values are loaded into the `ConfigLoader` source chain only; they do not mutate the process environment. Read dotenv-backed values from the typed config returned by `load()`, not from `std::env`. Malformed auto-discovered `.env` files are logged and skipped so optional local developer files do not prevent startup.

## Secrets

Use [`SecretString`](https://docs.rs/rskit-config/latest/rskit_config/struct.SecretString.html) for credentials, tokens, private keys, and other secret fields. It deserializes from config sources but masks `Debug`, `Display`, and serialization output; call `expose()` only at the boundary that needs the plaintext. Secrets keep this redaction across read, write, and watch.

## `ServiceConfig` fields

| Field | Type | Default | Description |
|-------|------|---------|-------------|
| `name` | `String` | `"service"` | Service name |
| `environment` | `Environment` | `Development` | Deployment environment |
| `version` | `String` | package version | Service version |
| `address` | `String` | `"0.0.0.0"` | Service bind address |
| `port` | `u16` | `50051` | Service port |
| `debug` | `bool` | `false` | Debug mode |
| `logging` | `LoggingConfig` | | Logging (level, format, output) |

> `LoggingConfig`, `LogFormat`, and `LogOutput` are owned by
> [`rskit-logging`](https://docs.rs/rskit-logging) and re-exported here under the
> `validate` feature for convenience. They are plain `serde` data and carry no
> `tracing` dependency, so loading config never links the logging subscriber
> stack.

## See also

[Main repository README](https://github.com/kbukum/rskit)
