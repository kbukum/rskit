# rskit-http — Axum HTTP Server

Axum HTTP server with graceful shutdown, CORS, request-ID, and `Component` lifecycle.

[![CI](https://github.com/kbukum/rskit/actions/workflows/ci.yml/badge.svg)](https://github.com/kbukum/rskit/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/rskit-http.svg)](https://crates.io/crates/rskit-http)
[![docs.rs](https://docs.rs/rskit-http/badge.svg)](https://docs.rs/rskit-http)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![MSRV: 1.85](https://img.shields.io/badge/MSRV-1.85-orange.svg)](https://blog.rust-lang.org/2025/02/20/Rust-1.85.0.html)

## Features

- Built on Axum with optional HTTP/2 cleartext (h2c)
- `HttpServerBuilder` fluent API with CORS, request-ID, and tracing middleware
- `RequestId` / `CorrelationId` extractors for distributed tracing
- `HttpError` converts `AppError` into Axum responses
- `health_router()` returns JSON health from the component registry
- Graceful shutdown via `CancellationToken`
- Implements `rskit-bootstrap::Component` lifecycle

## Usage

```toml
[dependencies]
rskit-http = "0.1"
```

```rust
use axum::{Router, routing::get, Json};
use rskit_http::{HttpServer, HttpServerBuilder, HttpServerConfig};
use tokio_util::sync::CancellationToken;
use serde_json::json;

let config = HttpServerConfig {
    host: "0.0.0.0".into(),
    port: 3000,
    ..Default::default()
};

let cancel = CancellationToken::new();
let app = Router::new().route("/hello", get(|| async { Json(json!({"ok": true})) }));

let server = HttpServerBuilder::new(config, cancel)
    .with_router(app)
    .with_request_id()
    .with_cors()
    .build();
```

## See Also

[Main repository README](https://github.com/kbukum/rskit)
