# rskit-database

Vendor-neutral database contracts with an explicit backend registry and an
in-memory backend for tests and local development.

```rust,no_run
use rskit_database::{DatabaseConfig, DatabaseQuery, DatabaseRegistry, register_memory};

# async fn example() -> rskit_errors::AppResult<()> {
let mut registry = DatabaseRegistry::new();
register_memory(&mut registry)?;

let db = registry.build(&DatabaseConfig::default()).await?;
db.execute(DatabaseQuery::new("SELECT 1")).await?;
# Ok(())
# }
```
