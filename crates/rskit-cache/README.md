# rskit-cache

Cache abstraction with an in-memory default, explicit backend registry, typed
JSON store, and optional Redis support.

The core crate is backend-neutral. A new `CacheRegistry` starts empty; call
`register_memory` or `register_redis` explicitly before selecting a backend from
configuration. Redis is behind the `redis` feature and is never registered by
import side effects.

## Installation

```toml
[dependencies]
rskit-cache = "0.1"

# Optional Redis backend
rskit-cache = { version = "0.1", features = ["redis"] }
```

## In-memory backend

```rust,no_run
use rskit_cache::{CacheConfig, CacheRegistry, MemoryConfig, TypedStore, register_memory};

# async fn example() -> rskit_errors::AppResult<()> {
let mut registry = CacheRegistry::new();
register_memory(&mut registry)?;

let cache = registry
    .build(&CacheConfig {
        backend: "memory".into(),
        memory: MemoryConfig::default(),
        #[cfg(feature = "redis")]
        redis: Default::default(),
    })
    .await?;

let store = TypedStore::<String>::new(cache, "sessions");
store.set("s1", &"user-1".to_string(), None).await?;
# Ok(())
# }
```

## Redis backend

```rust,no_run
#[cfg(feature = "redis")]
use rskit_cache::{CacheConfig, CacheRegistry, RedisConfig, register_redis};

# #[cfg(feature = "redis")]
# async fn example() -> rskit_errors::AppResult<()> {
let mut registry = CacheRegistry::new();
register_redis(&mut registry)?;

let cache = registry
    .build(&CacheConfig {
        backend: "redis".into(),
        memory: Default::default(),
        redis: RedisConfig::default(),
    })
    .await?;
# Ok(())
# }
```

## Public API

- `CacheBackend` — async get/set/delete/exists operations with TTL support.
- `CacheRegistry` — injected registry for config-driven backend selection.
- `MemoryCache` — lean default backend for local use and tests.
- `TypedStore<T>` — JSON-serialized typed values with key prefixes.
- `RedisClient` — optional Redis implementation when the `redis` feature is enabled.
