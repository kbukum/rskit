# rskit-server

Service-facing server abstractions for rskit.

## Responsibilities

- owns the HTTP server abstraction (`HttpServerBuilder`, `HttpServerConfig`)
- defines the canonical interceptor ordering: tracing -> logging -> auth -> validation -> handler -> metrics
- exposes health routers and lifecycle-managed startup/shutdown
- remains the home for shared server lifecycle pieces consumed by `rskit-grpc`
- keeps TLS policy ownership for service-facing transports; `rskit-http` stays transport-only

## Usage

```toml
[dependencies]
rskit-server = "0.1"
```

```rust,ignore
use axum::{Router, routing::get};
use rskit_server::{HttpServerBuilder, HttpServerConfig};
use tokio_util::sync::CancellationToken;

let server = HttpServerBuilder::new(HttpServerConfig::default(), CancellationToken::new())
    .with_router(Router::new().route("/healthz", get(|| async { "ok" })))
    .with_tracing()
    .with_request_id()
    .build();
```

## TLS policy

`GrpcServerConfig::tls` uses tonic/rustls modern defaults: TLS 1.3 preferred, TLS 1.2
minimum, and no legacy protocol support.
