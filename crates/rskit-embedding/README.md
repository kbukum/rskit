# rskit-embedding — Embedding Providers

Embedding provider abstractions for vector search with distance utilities.

[![CI](https://github.com/kbukum/rskit/actions/workflows/ci.yml/badge.svg)](https://github.com/kbukum/rskit/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/rskit-embedding.svg)](https://crates.io/crates/rskit-embedding)
[![docs.rs](https://docs.rs/rskit-embedding/badge.svg)](https://docs.rs/rskit-embedding)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![MSRV: 1.85](https://img.shields.io/badge/MSRV-1.85-orange.svg)](https://blog.rust-lang.org/2025/02/20/Rust-1.85.0.html)

## Features

- `EmbeddingProvider` trait — `embed`, `embed_batch`, and `dimensions`
- `OpenAiEmbeddingProvider` — works with OpenAI, Azure, llama.cpp, vLLM
- `Embedding` struct with optional text and model metadata
- Distance functions: `cosine_similarity`, `euclidean_distance`, `dot_product`
- Pooling: `mean_pooling`, `max_pooling`

## Usage

```toml
[dependencies]
rskit-embedding = "0.1"
```

```rust
use rskit_embedding::{cosine_similarity, mean_pooling, Embedding};

let a = vec![1.0_f32, 0.0, 0.5];
let b = vec![0.9, 0.1, 0.4];

let sim = cosine_similarity(&a, &b);
println!("cosine similarity: {sim:.4}");

let pooled = mean_pooling(&[a, b]).unwrap();
println!("mean-pooled dims: {}", pooled.len());

let emb = Embedding::new(vec![0.1, 0.2, 0.3])
    .with_text("hello")
    .with_model("text-embedding-3-small");
println!("dimensions: {}", emb.dimensions());
```

## See Also

[Main repository README](https://github.com/kbukum/rskit)
