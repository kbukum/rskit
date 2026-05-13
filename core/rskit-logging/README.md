# rskit-logging

Production-ready structured logging built on the [tracing](https://docs.rs/tracing) ecosystem.

[![CI](https://github.com/kbukum/rskit/actions/workflows/ci.yml/badge.svg)](https://github.com/kbukum/rskit/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/rskit-logging.svg)](https://crates.io/crates/rskit-logging)
[![docs.rs](https://docs.rs/rskit-logging/badge.svg)](https://docs.rs/rskit-logging)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![MSRV: 1.85](https://img.shields.io/badge/MSRV-1.85-orange.svg)](https://blog.rust-lang.org/2025/02/20/Rust-1.85.0.html)

## Features

- Structured JSON / pretty console output
- Sensitive data masking (**on by default**)
- Rate-based log sampling (burst + thereafter)
- Per-module log level overrides via `EnvFilter`
- OpenTelemetry Logs bridge (OTLP export, behind `otlp` feature flag)
- Unified log schema (consistent across gokit, pykit, rskit)
- Drop-based guard ensures all buffered logs are flushed on shutdown
- `RUST_LOG` env-filter support

## Installation

```toml
[dependencies]
rskit-logging = "0.1"

# With OTLP export support
rskit-logging = { version = "0.1", features = ["otlp"] }
```

## Quick Start

```rust
use rskit_logging::init_logging;
use rskit_config::LoggingConfig;

fn main() {
    let cfg = LoggingConfig::default();
    let _guard = init_logging(&cfg);
    // _guard must stay alive for the duration of the program

    tracing::info!(service = "my-svc", "server started");

    // Sensitive data is automatically redacted when using masking init
    // (see Masking section below)
}
```

## Configuration

rskit-logging integrates with `rskit-config` for configuration-driven setup. All logging options come from `LoggingConfig`:

```yaml
logging:
  level: info           # trace | debug | info | warn | error
  format: json          # json | console
```

### Init Functions

| Function | Description |
|----------|-------------|
| `init_logging(cfg)` | Basic init from `LoggingConfig` |
| `init_logging_env()` | Init from `RUST_LOG` only (no config needed) |
| `init_logging_with_masking(cfg, masking_cfg)` | Init with sensitive data masking |
| `init_logging_with_options(cfg, sampling, module_levels, masking)` | Init with sampling + module overrides |
| `init_logging_full(cfg, sampling, module_levels, masking, otlp, name, env, ver)` | Full init with all features (requires `otlp` feature) |

### Full Configuration Example

```rust
use std::collections::HashMap;
use rskit_logging::{
    init_logging_full,
    MaskingConfig, SamplingConfig,
};
use rskit_logging::otlp::OtlpConfig;  // requires "otlp" feature
use rskit_config::LoggingConfig;

fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let cfg = LoggingConfig {
        level: "info".into(),
        format: rskit_config::LogFormat::Json,
        ..Default::default()
    };

    let sampling = SamplingConfig {
        enabled: true,
        initial_rate: 100,
        thereafter_rate: 10,
    };

    let mut module_levels = HashMap::new();
    module_levels.insert("sqlx".to_string(), "warn".to_string());
    module_levels.insert("rdkafka".to_string(), "off".to_string());

    let otlp = OtlpConfig {
        enabled: true,
        endpoint: "http://otel-collector:4317".to_string(),
        protocol: "grpc".to_string(),
        ..Default::default()
    };

    let masking = MaskingConfig {
        enabled: true,
        ..Default::default()
    };

    let _guard = init_logging_full(
        &cfg,
        Some(&sampling),
        Some(&module_levels),
        Some(&masking),
        Some(&otlp),
        "my-service",
        "production",
        "1.0.0",
    )?;

    tracing::info!(service = "my-service", "started");
    Ok(())
}
```

## Masking

Masking is **enabled by default** in `MaskingConfig`. The `DefaultMasker` operates at the output layer via `MaskingMakeWriter`, redacting sensitive data from complete log lines before they reach any sink.

### Setup

```rust
use rskit_logging::{init_logging_with_masking, MaskingConfig};
use rskit_config::LoggingConfig;

let cfg = LoggingConfig::default();
let masking = MaskingConfig::default(); // enabled: true
let _guard = init_logging_with_masking(&cfg, &masking);

// Sensitive fields are now redacted in output
tracing::info!(password = "hunter2", "user login");
// output: password=[REDACTED]
```

### Default Masked Fields

| # | Field Name | Description |
|---|-----------|-------------|
| 1 | `password` | User passwords |
| 2 | `secret` | Generic secrets |
| 3 | `token` | Generic tokens |
| 4 | `api_key` | API keys |
| 5 | `apikey` | API keys (alternate) |
| 6 | `api-key` | API keys (hyphenated) |
| 7 | `authorization` | Auth headers |
| 8 | `auth_token` | Authentication tokens |
| 9 | `access_token` | OAuth access tokens |
| 10 | `refresh_token` | OAuth refresh tokens |
| 11 | `private_key` | Private keys |
| 12 | `ssn` | Social Security numbers |
| 13 | `credit_card` | Credit card numbers |
| 14 | `card_number` | Card numbers (alternate) |
| 15 | `cvv` | Card verification values |
| 16 | `pin` | Personal identification numbers |

### Value Patterns

These patterns detect sensitive data regardless of field name:

| # | Pattern | Example Input | Masked Output |
|---|---------|---------------|---------------|
| 1 | Bearer token | `Bearer abc123def` | `Bearer [REDACTED]` |
| 2 | JWT | `eyJhbGci...payload...sig` | `[JWT_REDACTED]` |
| 3 | AWS Access Key | `AKIAIOSFODNN7EXAMPLE` | `[AWS_KEY_REDACTED]` |
| 4 | Credit Card | `4111-1111-1111-1234` | `****-****-****-1234` |
| 5 | SSN | `123-45-6789` | `***-**-****` |
| 6 | Email | `user@example.com` | `***@***.***` |
| 7 | Hex Secret (32+) | `a1b2c3d4e5f6...` (32+ hex chars) | `[HEX_REDACTED]` |

### Adding Custom Fields and Patterns

```rust
let masking = MaskingConfig {
    enabled: true,
    field_names: vec!["my_internal_token".into(), "employee_id".into()],
    value_patterns: vec![r"MYSVC_[A-Za-z0-9]{32}".into()],
    replacement: "[REDACTED]".into(),
};
let _guard = init_logging_with_masking(&cfg, &masking);
```

### Output-Level Masking

Unlike gokit and pykit (which mask at the field level), rskit masks at the **output writer** level. The `MaskingMakeWriter` wraps the underlying `io::Write` and applies both field-name regex patterns (matching JSON `"field":"value"` and text `field=value` formats) and value-pattern regexes to complete log lines. This ensures nothing leaks regardless of how fields are formatted.

```rust
use std::sync::Arc;
use rskit_logging::masking::{DefaultMasker, Masker, MaskingMakeWriter};

let masker: Arc<dyn Masker> = Arc::new(DefaultMasker::default());
let writer = MaskingMakeWriter::new(std::io::stdout, masker);
```

## Sampling

Sampling reduces log volume in high-throughput services. When enabled, each log level gets an independent counter per one-second window:

1. **Burst** — the first `initial_rate` events per second per level pass through unconditionally.
2. **Thereafter** — after the burst, only every `thereafter_rate`-th event is kept.

```rust
use rskit_logging::SamplingConfig;

let sampling = SamplingConfig {
    enabled: true,
    initial_rate: 100,     // allow first 100/sec per level
    thereafter_rate: 10,   // then keep every 10th
};
```

> **When to use:** Enable sampling on hot-path services producing thousands of log events per second. Leave disabled for low-volume services or during debugging.

The `SamplingLayer` implements `tracing_subscriber::Layer` and uses `event_enabled()` to drop excess events. Counters are protected by `parking_lot::Mutex` for minimal contention.

## Module Levels

Override the global log level for specific modules using `tracing_subscriber::EnvFilter` directives. Useful for silencing noisy dependencies or enabling debug output for a single crate.

```rust
use std::collections::HashMap;
use rskit_logging::init_logging_with_options;
use rskit_config::LoggingConfig;

let cfg = LoggingConfig::default();

let mut module_levels = HashMap::new();
module_levels.insert("sqlx".to_string(), "warn".to_string());
module_levels.insert("rdkafka".to_string(), "off".to_string());
module_levels.insert("hyper".to_string(), "error".to_string());

let _guard = init_logging_with_options(&cfg, None, Some(&module_levels), None);
// Generates filter: "info,hyper=error,rdkafka=off,sqlx=warn"
```

The `build_env_filter()` function merges the base level with per-module overrides into a single `EnvFilter`. When `RUST_LOG` is set, it takes precedence over config.

```rust
use rskit_logging::module_levels::build_env_filter;

let filter = build_env_filter("info", &module_levels);
// Equivalent to: RUST_LOG="info,sqlx=warn,rdkafka=off,hyper=error"
```

## OTLP Export

The OpenTelemetry Logs bridge sends tracing events to an OTLP collector. It uses `opentelemetry-appender-tracing` to convert every `tracing::Event` into an OTel log record.

> **Feature gate:** OTLP requires the `otlp` cargo feature.

```toml
[dependencies]
rskit-logging = { version = "0.1", features = ["otlp"] }
```

### Setup

```rust
use rskit_logging::otlp::OtlpConfig;

let otlp = OtlpConfig {
    enabled: true,
    endpoint: "http://otel-collector:4317".to_string(),
    protocol: "grpc".to_string(),    // "grpc" | "http"
    insecure: false,
    headers: HashMap::new(),
};
```

### Full Init with OTLP

```rust
let _guard = rskit_logging::init_logging_full(
    &cfg,
    Some(&sampling),       // optional sampling
    Some(&module_levels),  // optional module overrides
    None,                  // optional masking config
    Some(&otlp),           // OTLP config
    "my-service",          // service name
    "production",          // environment
    "1.0.0",               // version
)?;
```

### Subscriber Stack

When using `init_logging_full`, the subscriber layers are composed as:

1. `EnvFilter` — base level + per-module overrides
2. `SamplingLayer` (optional) — rate-based event sampling
3. Format layer — JSON or console output
4. `OpenTelemetryTracingBridge` (optional) — OTLP export

### Graceful Shutdown

The `LoggingGuard` must be held for the lifetime of your service. When dropped, it restores the previous subscriber. When OTLP is enabled, the `OtlpProvider::shutdown()` method flushes pending records:

```rust
fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let _guard = rskit_logging::init_logging_full(...)?;

    // ... application runs ...

    // _guard is dropped here → subscriber restored, OTLP flushed
    Ok(())
}
```

## Unified Schema

All three kits (gokit, pykit, rskit) share the same structured field names, defined in `rskit_logging::fields::names`:

| Field | Constant | Description |
|-------|----------|-------------|
| `service` | `fields::names::SERVICE` | Service name |
| `environment` | `fields::names::ENVIRONMENT` | Deployment environment |
| `version` | `fields::names::VERSION` | Service version |
| `component` | `fields::names::COMPONENT` | Logical component |
| `trace_id` | `fields::names::TRACE_ID` | Distributed trace ID |
| `span_id` | `fields::names::SPAN_ID` | Span ID within trace |
| `correlation_id` | `fields::names::CORRELATION_ID` | Cross-service correlation |
| `user_id` | `fields::names::USER_ID` | User identifier |
| `request_id` | `fields::names::REQUEST_ID` | HTTP request identifier |
| `duration_ms` | `fields::names::DURATION_MS` | Duration in milliseconds |

### Using Field Constants

```rust
use rskit_logging::fields::names::*;

tracing::info!(
    { SERVICE } = "order-svc",
    { ENVIRONMENT } = "production",
    { VERSION } = "1.2.3",
    { COMPONENT } = "checkout",
    "order placed"
);
```

## Custom Masker

Implement the `Masker` trait to provide your own masking logic:

```rust
use std::borrow::Cow;
use rskit_logging::masking::Masker;

struct MyMasker;

impl Masker for MyMasker {
    fn mask_value<'v>(&self, key: &str, value: &'v str) -> Cow<'v, str> {
        if key == "internal_id" {
            Cow::Borrowed("***")
        } else {
            Cow::Borrowed(value)
        }
    }

    fn mask_output<'v>(&self, line: &'v str) -> Cow<'v, str> {
        // Apply value-level patterns to complete log lines
        Cow::Borrowed(line)
    }
}
```

Use with `MaskingMakeWriter`:

```rust
use std::sync::Arc;
use rskit_logging::masking::MaskingMakeWriter;

let masker: Arc<dyn Masker> = Arc::new(MyMasker);
let writer = MaskingMakeWriter::new(std::io::stdout, masker);
```

## Convenience Re-exports

rskit-logging re-exports core tracing macros for convenience:

```rust
use rskit_logging::{info, warn, error, debug, trace, instrument};

#[instrument]
fn process_order(id: &str) {
    info!("processing order");
}
```

## API Reference

| Function / Type | Description |
|----------------|-------------|
| `init_logging(cfg)` | Basic subscriber init |
| `init_logging_env()` | Init from `RUST_LOG` only |
| `init_logging_with_masking(cfg, masking)` | Init with output masking |
| `init_logging_with_options(cfg, sampling, modules, masking)` | Init with sampling + module levels |
| `init_logging_full(...)` | Full init with OTLP (`otlp` feature) |
| `init_global(cfg)` | Set global subscriber |
| `init_global_with_masking(cfg, masking)` | Global subscriber with masking |
| `init_global_with_options(...)` | Global subscriber with all options |
| `LoggingGuard` | Drop guard — hold for program lifetime |
| `GlobalLoggingGuard` | Drop guard for global subscriber |
| `MaskingConfig` | Masking configuration |
| `DefaultMasker` | Built-in masker with PII/secret patterns |
| `Masker` (trait) | Interface for custom maskers |
| `MaskingMakeWriter` | Writer wrapper that masks output |
| `SamplingConfig` | Sampling configuration |
| `SamplingLayer` | Tracing layer for rate-based sampling |
| `ModuleLevelsConfig` | Per-module level overrides |
| `build_env_filter(level, modules)` | Build `EnvFilter` from config |
| `OtlpConfig` | OTLP export configuration (`otlp` feature) |
| `OtlpProvider` | OTel LoggerProvider manager (`otlp` feature) |

## See Also

- [Main repository README](https://github.com/kbukum/rskit)
- [API documentation on docs.rs](https://docs.rs/rskit-logging)
