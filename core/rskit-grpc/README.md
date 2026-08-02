# rskit-grpc

gRPC transport and status mapping entrypoint for rskit.

## Always Available

- Status mapping between `tonic::Status` and `rskit_errors::AppError`

## Features

- `client` (default): lazy tonic channels with TLS-aware dialing and optional discovery
- `discovery`: enables `DiscoveryChannel`

## Usage

```toml
[dependencies]
rskit-grpc = { version = "0.2.0-alpha.3", features = ["client", "discovery"] }
```

```rust,ignore
use rskit_grpc::{GrpcChannel, GrpcClientConfig};

let client = GrpcChannel::new(GrpcClientConfig::new("localhost:50051"));
```

## Discovery reconnect contract

`DiscoveryChannel` only advances its cached target after a connection succeeds. Failed background reconnect attempts leave the previously connected target in place so watcher/poll updates keep retrying the same discovered endpoint until it becomes reachable.

## TLS policy

Client TLS uses `rskit_security::TlsConfig` so CA bundles, server-name overrides, and client certificate/key material share the same shape as other transports. `skip_verify` is rejected for gRPC clients. tonic/rustls provides the protocol defaults: TLS 1.3 preferred, TLS 1.2 minimum, and no legacy protocols.
