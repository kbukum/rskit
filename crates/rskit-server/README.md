# rskit-server — gRPC Server Component

`tonic`-backed gRPC server that integrates with `rskit-bootstrap`'s `Component` lifecycle.

[![CI](https://github.com/kbukum/rskit/actions/workflows/ci.yml/badge.svg)](https://github.com/kbukum/rskit/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/rskit-server.svg)](https://crates.io/crates/rskit-server)
[![docs.rs](https://docs.rs/rskit-server/badge.svg)](https://docs.rs/rskit-server)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![MSRV: 1.85](https://img.shields.io/badge/MSRV-1.85-orange.svg)](https://blog.rust-lang.org/2025/02/20/Rust-1.85.0.html)

## Features

- `GrpcServerBuilder` fluent API
- Optional gRPC reflection support
- Optional health check endpoint
- Graceful shutdown via `CancellationToken`
- Validator-backed `GrpcServerConfig`

## Usage

```toml
[dependencies]
rskit-server = "0.1"
```

```rust
use rskit_server::{GrpcServerBuilder, GrpcServerConfig};

let server = GrpcServerBuilder::new(
        GrpcServerConfig::builder().port(50051).build()?,
        cancel.clone(),
    )
    .with_name("my-service")
    .add_service(MyServiceServer::new(impl_))
    .with_health_check()
    .build()?;

registry.register(Arc::new(server)).await;
```

## See Also

[Main repository README](https://github.com/kbukum/rskit)
