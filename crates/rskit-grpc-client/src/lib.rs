#![warn(missing_docs)]
#![doc = include_str!("../README.md")]

//! # rskit-grpc-client
//!
//! A tonic-based gRPC client with lazy connection management, discovery integration,
//! and bidirectional error mapping.
//!
//! Mirrors [`gokit/grpc/client`] and [`pykit-grpc`] patterns for Rust.
//!
//! ## Features
//!
//! - **Lazy connection**: Channel only connects on first use
//! - **Configurable**: Timeouts, keepalive, message sizes, TLS
//! - **Error mapping**: Seamless conversion between tonic `Status` and `AppError`
//! - **Discovery support**: Dynamic service resolution (optional `discovery` feature)
//! - **Async/await**: Full async support with tokio
//!
//! ## Example
//!
//! ```ignore
//! use rskit_grpc_client::{GrpcChannel, GrpcClientConfig};
//!
//! #[tokio::main]
//! async fn main() {
//!     let config = GrpcClientConfig::new("localhost:50051");
//!     let channel = GrpcChannel::new(config);
//!
//!     // Channel connects lazily on first use
//!     let ch = channel.connected_channel().await.expect("failed to connect");
//!
//!     // Use ch with generated gRPC client stubs...
//! }
//! ```
//!
//! [`gokit/grpc/client`]: https://github.com/kbukum/gokit/tree/main/grpc/client
//! [`pykit-grpc`]: https://github.com/kbukum/pykit/tree/main/packages/pykit-grpc

/// gRPC client configuration.
pub mod config;
/// gRPC channel wrapper with lazy connection.
pub mod channel;
/// Error mapping between tonic Status and AppError.
pub mod errors;

#[cfg(feature = "discovery")]
/// Service discovery integration (requires `discovery` feature).
pub mod discovery;

pub use channel::GrpcChannel;
pub use config::GrpcClientConfig;
pub use errors::{app_error_to_status, status_to_app_error};

#[cfg(feature = "discovery")]
pub use discovery::DiscoveryChannel;

