# rskit-cache-redis

Redis adapter for `rskit-cache`. Register it explicitly with a `CacheRegistry`.

```rust,no_run
use rskit_cache::CacheRegistry;
use rskit_cache_redis::{Config, register};

# fn example() -> rskit_errors::AppResult<()> {
let mut registry = CacheRegistry::new();
register(&mut registry, Config::default())?;
# Ok(())
# }
```
