# rskit-vectorstore — Vector Similarity Search

Vector store abstractions with Qdrant and in-memory implementations.

[![CI](https://github.com/kbukum/rskit/actions/workflows/ci.yml/badge.svg)](https://github.com/kbukum/rskit/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/rskit-vectorstore.svg)](https://crates.io/crates/rskit-vectorstore)
[![docs.rs](https://docs.rs/rskit-vectorstore/badge.svg)](https://docs.rs/rskit-vectorstore)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![MSRV: 1.85](https://img.shields.io/badge/MSRV-1.85-orange.svg)](https://blog.rust-lang.org/2025/02/20/Rust-1.85.0.html)

## Features

- `VectorStore` trait — `ensure_collection`, `upsert`, `search`, `delete`
- `InMemoryVectorStore` for testing (cosine-similarity linear scan)
- `QdrantVectorStore` for production (feature-gated)
- `PointPayload` — key-value metadata alongside vectors
- `SearchFilter` — optional `must_match` field filtering
- `SearchResult` with id, score, and payload

## Usage

```toml
[dependencies]
rskit-vectorstore = "0.1"
```

```rust
use rskit_vectorstore::{VectorStore, InMemoryVectorStore, PointPayload};

async fn example() {
    let store = InMemoryVectorStore::new();
    store.ensure_collection("docs", 3).await.unwrap();

    let payload = PointPayload::new().with_field("title", "Doc 1");
    store.upsert("docs", "d1", vec![1.0, 0.0, 0.5], payload).await.unwrap();

    let results = store.search("docs", vec![1.0, 0.0, 0.5], 5, None).await.unwrap();
    for r in &results {
        println!("id={} score={:.3}", r.id, r.score);
    }
}
```

## See Also

[Main repository README](https://github.com/kbukum/rskit)
