# rskit-vectorstore

Vector similarity abstraction with an in-memory default, explicit backend
registry, canonical metrics (`cosine`, `dot`, `l2`), metadata filters, and an
optional Qdrant backend.

The core path is deterministic and local. Qdrant is behind the `qdrant` feature
and is registered only when the application calls `register_qdrant`.

## Installation

```toml
[dependencies]
rskit-vectorstore = "0.1"

# Optional Qdrant backend
rskit-vectorstore = { version = "0.1", features = ["qdrant"] }
```

## In-memory backend

```rust,no_run
use rskit_vectorstore::{
    PointPayload, VectorStore, VectorStoreRegistry, register_memory,
};

# async fn example() -> rskit_errors::AppResult<()> {
let mut registry = VectorStoreRegistry::new();
register_memory(&mut registry)?;

let store = registry.build("memory")?;
store.ensure_collection("docs", 3).await?;

let payload = PointPayload::new().with_field("tenant", "acme");
store.upsert("docs", "d1", vec![1.0, 0.0, 0.5], payload).await?;
# Ok(())
# }
```

## Public API

- `VectorStore` — collection creation, upsert, search, and delete.
- `VectorStoreRegistry` — injected registry for config-driven backend selection.
- `InMemoryVectorStore` — lean default backend for tests and local workflows.
- `SimilarityMetric` — canonical `cosine`, `dot`, and `l2` names.
- `SearchFilter` and `FilterCondition` — metadata filtering for tenant-aware search.
- `QdrantVectorStore` — optional Qdrant backend when the `qdrant` feature is enabled.
