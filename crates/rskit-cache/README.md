# rskit-cache — Redis Client & Typed Store

Redis client with typed store, connection management, and `Component` lifecycle.

[![CI](https://github.com/kbukum/rskit/actions/workflows/ci.yml/badge.svg)](https://github.com/kbukum/rskit/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/rskit-cache.svg)](https://crates.io/crates/rskit-cache)
[![docs.rs](https://docs.rs/rskit-cache/badge.svg)](https://docs.rs/rskit-cache)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![MSRV: 1.85](https://img.shields.io/badge/MSRV-1.85-orange.svg)](https://blog.rust-lang.org/2025/02/20/Rust-1.85.0.html)

## Features

- `RedisClient` — async connection with pooling and automatic key prefixing
- `TypedStore<T>` — JSON-serialized typed cache backed by `RedisClient`
- String, hash, list, scan, and pub/sub operations
- TTL support for cache expiration
- Implements `rskit-bootstrap::Component` lifecycle

## Usage

```toml
[dependencies]
rskit-cache = "0.1"
```

```rust
use rskit_cache::{RedisClient, RedisConfig, TypedStore};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use std::time::Duration;

#[derive(Serialize, Deserialize)]
struct Session { user_id: String }

async fn example() {
    let config = RedisConfig {
        host: "127.0.0.1".into(),
        port: 6379,
        ..Default::default()
    };
    let client = Arc::new(RedisClient::new(config).await.unwrap());
    let store = TypedStore::<Session>::new(client, "sessions");

    store.set("s1", &Session { user_id: "u1".into() }, Some(Duration::from_secs(3600))).await.unwrap();
    let s: Option<Session> = store.get("s1").await.unwrap();
}
```

## See Also

[Main repository README](https://github.com/kbukum/rskit)
