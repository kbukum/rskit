# rskit-database — Async Database Pool

sqlx-based async database pool with repository pattern and slow-query logging.

[![CI](https://github.com/kbukum/rskit/actions/workflows/ci.yml/badge.svg)](https://github.com/kbukum/rskit/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/rskit-database.svg)](https://crates.io/crates/rskit-database)
[![docs.rs](https://docs.rs/rskit-database/badge.svg)](https://docs.rs/rskit-database)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![MSRV: 1.85](https://img.shields.io/badge/MSRV-1.85-orange.svg)](https://blog.rust-lang.org/2025/02/20/Rust-1.85.0.html)

## Features

- Multi-driver support (PostgreSQL, MySQL, SQLite) via `DbDriver`
- Connection pooling with configurable min/max, timeouts, and idle lifetime
- `Repository<T, ID>` async trait for CRUD operations
- `FindOpts` builder with limit, offset, order, and filter
- Automatic slow-query logging at WARN level
- Implements `rskit-bootstrap::Component` lifecycle

## Usage

```toml
[dependencies]
rskit-database = "0.1"
```

```rust
use rskit_database::{Database, DatabaseConfig, DbDriver, FindOpts};
use rskit_errors::AppResult;

async fn example() -> AppResult<()> {
    let config = DatabaseConfig {
        driver: DbDriver::Sqlite,
        database: ":memory:".into(),
        max_connections: 5,
        ..Default::default()
    };
    let db = Database::new(config).await?;
    // Use Repository<T, ID> trait for typed data access
    Ok(())
}
```

## See Also

[Main repository README](https://github.com/kbukum/rskit)
