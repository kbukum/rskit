# rskit-server

Service-facing server abstractions for rskit.

## Responsibilities

- owns the HTTP server abstraction (`HttpServerBuilder`, `HttpServerConfig`)
- defines the canonical interceptor ordering:
  tracing -> logging -> auth -> validation -> handler -> metrics
- exposes health routers and lifecycle-managed startup/shutdown
- remains the home for shared server lifecycle pieces consumed by `rskit-grpc`
- consumes `rskit-http` for HTTP transport policy and Tower layer construction
- serves HTTP directly over rustls when `HttpServerConfig::tls` is configured
- enforces request body limits, request timeout middleware, HTTP/1 header-read timeout,
  and HTTP/2 keepalive timeout from `HttpServerConfig`

## Usage

```toml
[dependencies]
rskit-server = "0.2.0-alpha.1"
```

```rust,ignore
use axum::{Router, routing::get};
use rskit_server::{HttpServerBuilder, HttpServerConfig};
use tokio_util::sync::CancellationToken;

let server = HttpServerBuilder::new(HttpServerConfig::default(), CancellationToken::new())
    .with_router(Router::new().route("/healthz", get(|| async { "ok" })))
    .build()?;
```

## TLS policy

`HttpServerConfig::tls` uses `rskit_security::TlsConfig` with `cert_file`
and `key_file` required for direct HTTPS serving. Client-only settings such as `skip_verify`,
`ca_file`, and `server_name` are rejected for server TLS.

`GrpcServerConfig::tls` uses tonic/rustls modern defaults. Server TLS prefers TLS 1.3,
permits TLS 1.2 as the compatibility floor, and does not enable legacy protocols.
