# rskit-grpc-client

A tonic-based gRPC client crate for rskit with lazy connection management, discovery integration, and bidirectional error mapping.

## Features

- **Lazy Connection**: Channel only connects on first use, improving startup time
- **Configurable**: Timeouts, keepalive, message sizes, TLS support
- **Error Mapping**: Seamless conversion between tonic `Status` and `AppError`
- **Discovery Support**: Dynamic service resolution via rskit-discovery (optional)
- **Async/Await**: Full async support with tokio

## Design

This crate mirrors the design patterns from:
- [`gokit/grpc/client`](https://github.com/kbukum/gokit/tree/main/grpc) (Go)
- [`pykit-grpc`](https://github.com/kbukum/pykit/tree/main/packages/pykit-grpc) (Python)

### Components

- **`GrpcClientConfig`**: Configuration struct with sensible defaults
  - Target address (e.g., "localhost:50051")
  - TLS settings
  - Timeouts (request and connection)
  - Keepalive settings
  - Message size limits

- **`GrpcChannel`**: Lazy-connecting channel wrapper
  - Wraps `tonic::transport::Channel`
  - Connects on first `connected_channel()` call
  - Supports connectivity checks with `is_ready()`
  - Graceful shutdown with `close()`

- **`DiscoveryChannel`**: Service discovery integration (with `discovery` feature)
  - Resolves service instances via `rskit-discovery::Discovery`
  - Automatically reconnects if target changes
  - Supports manual refresh with change detection

- **Error Mapping**:
  - `status_to_app_error()`: tonic Status → AppError
  - `app_error_to_status()`: AppError → tonic Status
  - Comprehensive mapping of gRPC codes to rskit error codes

## Usage

### Basic Usage

```no_run
use rskit_grpc_client::{GrpcChannel, GrpcClientConfig};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create a config
    let config = GrpcClientConfig::new("example.com:50051");
    
    // Create a channel (no connection yet)
    let channel = GrpcChannel::new(config);
    
    // Get a connected channel (connects on first call)
    let tonic_channel = channel.connected_channel().await?;
    
    // Use with generated gRPC client stubs
    // let mut client = MyServiceClient::new(tonic_channel);
    
    Ok(())
}
```

### Custom Configuration

```rust
use std::time::Duration;
use rskit_grpc_client::{GrpcChannel, GrpcClientConfig};

let config = GrpcClientConfig {
    target: "api.example.com:9090".to_string(),
    tls: true,
    timeout: Duration::from_secs(60),
    connect_timeout: Duration::from_secs(20),
    keepalive_interval: Some(Duration::from_secs(30)),
    keepalive_timeout: Some(Duration::from_secs(10)),
    max_message_size: 10 * 1024 * 1024, // 10 MB
    max_send_message_size: 10 * 1024 * 1024,
};

let channel = GrpcChannel::new(config);
```

### With Discovery

Enable the `discovery` feature:

```toml
[dependencies]
rskit-grpc-client = { path = "...", features = ["discovery"] }
```

Then use:

```ignore
use std::sync::Arc;
use rskit_grpc_client::{DiscoveryChannel, GrpcClientConfig};
use rskit_discovery::Discovery;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let discovery: Arc<dyn Discovery> = get_discovery_provider();
    
    let config = GrpcClientConfig::new("localhost:50051"); // fallback
    let mut channel = DiscoveryChannel::new(discovery, "my-service", config);
    
    // Resolves service and connects
    let tonic_channel = channel.channel().await?;
    
    // Check for updates periodically
    if channel.refresh().await? {
        println!("Service target changed!");
    }
    
    channel.close().await?;
    Ok(())
}
```

### Error Handling

```rust
use rskit_grpc_client::status_to_app_error;
use tonic::Status;

let status = Status::not_found("user not found");
let app_error = status_to_app_error(status);

println!("Error code: {:?}", app_error.code);
println!("Message: {}", app_error.message);
println!("Retryable: {}", app_error.retryable);
```

## Error Mapping

The crate automatically maps between tonic gRPC status codes and rskit error codes:

| gRPC Code | rskit ErrorCode | HTTP Status |
|-----------|-----------------|-------------|
| NOT_FOUND | NotFound | 404 |
| INVALID_ARGUMENT | InvalidInput | 400 |
| UNAVAILABLE | ServiceUnavailable | 503 |
| UNAUTHENTICATED | Unauthorized | 401 |
| PERMISSION_DENIED | Forbidden | 403 |
| DEADLINE_EXCEEDED | Timeout | 408 |
| RESOURCE_EXHAUSTED | RateLimited | 429 |
| FAILED_PRECONDITION | Conflict | 409 |
| ALREADY_EXISTS | AlreadyExists | 409 |

## Defaults

- **Target**: `localhost:50051`
- **TLS**: Disabled (insecure)
- **Timeout**: 30 seconds
- **Connect Timeout**: 10 seconds
- **Keepalive Interval**: 30 seconds
- **Keepalive Timeout**: 10 seconds
- **Max Message Size**: 4 MB (both send and receive)

## Testing

```bash
cd crates/rskit-grpc-client
cargo test
cargo clippy -- -D warnings
cargo doc --open
```

## Integration with rskit

This crate integrates with:
- **rskit-errors**: AppError and ErrorCode
- **rskit-discovery**: Service discovery (optional feature)
- **tonic**: gRPC transport and protocol
- **tokio**: Async runtime

See the workspace Cargo.toml for version constraints.
