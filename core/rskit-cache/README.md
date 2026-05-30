# rskit-cache

Cache abstraction with built-in local stores, explicit store registry, and
typed JSON store. Remote infrastructure adapters live in `contrib/` crates.

```rust,no_run
use rskit_cache::{CacheConfig, CacheRegistry, MemoryConfig, TypedStore, register_memory};

# async fn example() -> rskit_errors::AppResult<()> {
let mut registry = CacheRegistry::new();
register_memory(&mut registry)?;

let cache = registry
    .build(&CacheConfig {
        store: "memory".into(),
        key_prefix: None,
        memory: MemoryConfig::default(),
    })
    .await?;

let store = TypedStore::<String>::new(cache, "sessions");
store.set("s1", &"user-1".to_string(), None).await?;
# Ok(())
# }
```
