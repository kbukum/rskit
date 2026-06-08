# rskit-vectorstore-qdrant

Qdrant adapter for `rskit-vectorstore`. Register it explicitly with a `VectorStoreRegistry`. Use `rskit_util::SecretString` for API keys; URLs with embedded credentials, query parameters, unsafe schemes, metadata endpoints, or link-local address literals are rejected before the Qdrant client is built. Runtime vector operation limits come from `VectorStoreConfig::limits`, matching the core in-memory backend. Payload and filter values are converted through the typed scalar contract; unsupported values fail closed instead of being silently dropped.

```rust,no_run
use rskit_vectorstore::VectorStoreRegistry;
use rskit_vectorstore_qdrant::{Config, register};

# fn example() -> rskit_errors::AppResult<()> {
let mut registry = VectorStoreRegistry::new();
register(&mut registry, Config::new("http://localhost:6334"))?;
# Ok(())
# }
```
