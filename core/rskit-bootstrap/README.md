# rskit-bootstrap — Service Lifecycle

Typestate `App<S,C>` builder, `Component` trait, ordered `Registry`, and `Health` reporting.

[![CI](https://github.com/kbukum/rskit/actions/workflows/ci.yml/badge.svg)](https://github.com/kbukum/rskit/actions/workflows/ci.yml) [![crates.io](https://img.shields.io/crates/v/rskit-bootstrap.svg)](https://crates.io/crates/rskit-bootstrap) [![docs.rs](https://docs.rs/rskit-bootstrap/badge.svg)](https://docs.rs/rskit-bootstrap) [![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/kbukum/rskit/blob/main/LICENSE) [![MSRV: 1.91](https://img.shields.io/badge/MSRV-1.91-orange.svg)](https://github.com/kbukum/rskit/blob/main/core/Cargo.toml)

## Features

- Compile-time lifecycle ordering via typestate pattern
- Reverse-order graceful shutdown
- Cancellation via `tokio_util::sync::CancellationToken`
- `HealthStatus` with three states: `Healthy`, `Degraded`, `Unhealthy`
- `Component` trait for pluggable lifecycle-managed services

## Usage

```toml
[dependencies]
rskit-bootstrap = "0.1.0-alpha.1"
```

```rust
use rskit_bootstrap::{AppBuilder, Component, Health};

AppBuilder::new(my_config)
    .build()?
    .before_start(|_cancel| async move {
        tracing::info!("starting");
        Ok(())
    })
    .run()
    .await?;
```

## See Also

[Main repository README](https://github.com/kbukum/rskit)
