# rskit-vectorstore

Vector similarity abstraction with an in-memory default, explicit backend registry, canonical metrics, typed scalar metadata filters, and configurable limits for search size, vector dimensions, payload/filter scalar byte size, and filter complexity. The registry-level `VectorStoreConfig::limits` are the authoritative request bounds for every backend.
External backends live in `contrib/` adapter crates.

```rust,no_run
use rskit_vectorstore::{
    PointPayload, VectorStoreConfig, VectorStoreLimits, VectorStoreRegistry, register_memory,
};

# fn example() -> rskit_errors::AppResult<()> {
let mut registry = VectorStoreRegistry::new();
register_memory(&mut registry)?;

let config = VectorStoreConfig {
    limits: VectorStoreLimits::new().with_max_search_limit(100),
    ..VectorStoreConfig::default()
};
let store = registry.build(&config)?;
let payload = PointPayload::new().with_field("tenant", "acme");
# let _ = (store, payload);
# Ok(())
# }
```
