# rskit-cache

Cache abstraction with an in-memory default, explicit backend registry, and
typed JSON store. External backends live in `contrib/` adapter crates.

```rust,no_run
use rskit_cache::{CacheConfig, CacheRegistry, MemoryConfig, TypedStore, register_memory};

# async fn example() -> rskit_errors::AppResult<()> {
let mut registry = CacheRegistry::new();
register_memory(&mut registry)?;

let cache = registry
    .build(&CacheConfig {
        backend: "memory".into(),
        key_prefix: None,
        memory: MemoryConfig::default(),
    })
    .await?;

let store = TypedStore::<String>::new(cache, "sessions");
store.set("s1", &"user-1".to_string(), None).await?;
# Ok(())
# }
```
