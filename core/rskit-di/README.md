# rskit-di — Dependency Injection Container

Lightweight `Arc`-based runtime dependency injection container.

[![CI](https://github.com/kbukum/rskit/actions/workflows/ci.yml/badge.svg)](https://github.com/kbukum/rskit/actions/workflows/ci.yml) [![crates.io](https://img.shields.io/crates/v/rskit-di.svg)](https://crates.io/crates/rskit-di) [![docs.rs](https://docs.rs/rskit-di/badge.svg)](https://docs.rs/rskit-di) [![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE) [![MSRV: 1.85](https://img.shields.io/badge/MSRV-1.85-orange.svg)](https://blog.rust-lang.org/2025/02/20/Rust-1.85.0.html)

## Features

- Three registration modes: eager (`register`), lazy factory (`register_factory`), singleton (`register_singleton`)
- Type-keyed resolution via `TypeId` — no strings, no macros
- `Resolve<T>` trait for constructor-oriented workflows
- Thread-safe with `parking_lot::RwLock`
- `Closeable` trait for async resource cleanup
- Returns `AppResult` for idiomatic error handling

## Usage

```toml
[dependencies]
rskit-di = "0.1.0-alpha.1"
```

```rust
use std::sync::Arc;
use rskit_di::Container;

struct Config { db_url: String }

let container = Container::new();
container.register(Arc::new(Config { db_url: "postgres://...".into() }));

let cfg: Arc<Config> = container.resolve()?;
assert_eq!(cfg.db_url, "postgres://...");
# Ok::<(), rskit_errors::AppError>(())
```

## See Also

[Main repository README](https://github.com/kbukum/rskit)
