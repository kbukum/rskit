#![warn(missing_docs)]
#![doc = include_str!("../README.md")]

//! gRPC transport entrypoints for rskit.
//!
//! `rskit-grpc` owns the locked transport shape for client and server concerns:
//! - `client` feature: lazy channels, TLS-aware dialing, discovery integration.
//! - `server` feature: re-exports the lifecycle-managed tonic server types from
//!   `rskit-server` so services consume a single `grpc` namespace.
//! - Server interceptor contract: tracing -> logging -> auth -> validation ->
//!   handler -> metrics.

#[cfg(feature = "client")]
/// gRPC channel wrapper with lazy connection management.
pub mod channel;
#[cfg(feature = "client")]
/// gRPC client configuration.
pub mod config;
#[cfg(feature = "client")]
/// Error mapping between tonic [`Status`](tonic::Status) and [`AppError`](rskit_errors::AppError).
pub mod errors;

#[cfg(all(feature = "client", feature = "discovery"))]
/// Service discovery integration.
pub mod discovery;

#[cfg(feature = "client")]
pub use channel::GrpcChannel;
#[cfg(feature = "client")]
pub use config::{GrpcClientConfig, GrpcTlsConfig};
#[cfg(feature = "client")]
pub use errors::{app_error_to_status, status_to_app_error};

#[cfg(all(feature = "client", feature = "discovery"))]
pub use discovery::{DiscoveryChannel, DiscoveryChannelConfig};

#[cfg(feature = "server")]
pub use rskit_server::{ErrorLayer, GrpcServer, GrpcServerBuilder, GrpcServerConfig, TlsConfig};
