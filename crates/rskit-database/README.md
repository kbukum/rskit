# rskit-database

Database contracts, bounded pool configuration, tenant helpers, query parsing,
and repository traits with feature-gated SQLx backend support.

The crate keeps backend selection explicit. A new `DatabaseRegistry` starts
empty; applications register only the SQL drivers they compile in and then
construct `Database` from validated configuration.

## Installation

```toml
[dependencies]
rskit-database = { version = "0.1", features = ["postgres", "sqlx-any"] }

# Optional driver features:
# postgres, mysql, sqlite, sqlx-any
```

## Backend registration

```rust,no_run
use rskit_database::{Database, DatabaseConfig, DatabaseRegistry, DbDriver, register_postgres};

# async fn example() -> rskit_errors::AppResult<()> {
let mut registry = DatabaseRegistry::new();
register_postgres(&mut registry)?;

let config = DatabaseConfig {
    driver: DbDriver::Postgres,
    host: "localhost".into(),
    database: "app".into(),
    user: "app".into(),
    max_connections: 10,
    ..Default::default()
};

assert!(registry.contains(&config.driver));
let database = Database::new(config).await?;
# Ok(())
# }
```

## Public API

- `DatabaseConfig` — driver, DSN fields, TLS mode, timeouts, and pool bounds.
- `DatabaseRegistry` — injected registry for compiled-in SQL backends.
- `Database` — SQLx-backed async pool and component integration.
- `Repository<T, ID>` and `FindOpts` — typed repository contracts.
- `TenantScope` — tenant filter helper for row-scoped data isolation.
