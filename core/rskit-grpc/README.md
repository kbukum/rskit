# rskit-grpc

Aligned gRPC transport entrypoint for rskit.

## Features

- `client` (default): lazy tonic channels with TLS-aware dialing and optional discovery
- `server`: re-exports `GrpcServerBuilder`, `GrpcServerConfig`, and `ErrorLayer` from `rskit-server`
- `discovery`: enables `DiscoveryChannel`

## Usage

```toml
[dependencies]
rskit-grpc = { version = "0.1", features = ["client", "discovery", "server"] }
```

```rust,ignore
use rskit_grpc::{GrpcChannel, GrpcClientConfig, GrpcServerBuilder, GrpcServerConfig};

let client = GrpcChannel::new(GrpcClientConfig::new("localhost:50051"));
let server = GrpcServerBuilder::new(GrpcServerConfig::default()).build();
```

## Interceptor ordering

When the `server` feature is enabled, `rskit-grpc` re-exports `rskit-server`'s locked
service interceptor contract:

`tracing -> logging -> auth -> validation -> handler -> metrics`

## Discovery reconnect contract

`DiscoveryChannel` only advances its cached target after a connection succeeds. Failed
background reconnect attempts leave the previously connected target in place so watcher/poll
updates keep retrying the same discovered endpoint until it becomes reachable.

## TLS policy

Client TLS uses `rskit_security::TlsConfig` so CA bundles, server-name overrides,
and client certificate/key material share the same shape as other transports.
`skip_verify` is rejected for gRPC clients. tonic/rustls provides the protocol
defaults: TLS 1.3 preferred, TLS 1.2 minimum, and no legacy protocols.
