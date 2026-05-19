# rskit-vectorstore

Vector similarity abstraction with an in-memory default, explicit backend
registry, canonical metrics, and metadata filters. External backends live in
`contrib/` adapter crates.

```rust,no_run
use rskit_vectorstore::{
    PointPayload, VectorStoreConfig, VectorStoreRegistry, register_memory,
};

# fn example() -> rskit_errors::AppResult<()> {
let mut registry = VectorStoreRegistry::new();
register_memory(&mut registry)?;

let store = registry.build(&VectorStoreConfig::default())?;
let payload = PointPayload::new().with_field("tenant", "acme");
# let _ = (store, payload);
# Ok(())
# }
```
