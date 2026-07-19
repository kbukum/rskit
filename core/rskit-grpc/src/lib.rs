#![warn(missing_docs)]
#![doc = include_str!("../README.md")]

//! gRPC transport and status-mapping entrypoints for rskit.
//!
//! `rskit-grpc` always owns `tonic::Status` ↔ `rskit_errors::AppError` mapping so client
//! and server crates share one canonical transport boundary. With the `client` feature enabled,
//! it also owns lazy channels, TLS-aware dialing, and optional discovery integration.
//!
//! Lifecycle-managed gRPC servers are owned by `rskit-server`.

#[cfg(feature = "client")]
/// gRPC channel wrapper with lazy connection management.
pub mod channel;
#[cfg(feature = "client")]
/// gRPC client configuration.
pub mod config;
/// Error mapping between tonic [`Status`](tonic::Status) and [`AppError`](rskit_errors::AppError).
pub mod errors;

#[cfg(all(feature = "client", feature = "discovery"))]
/// Service discovery integration.
pub mod discovery;

#[cfg(feature = "client")]
pub use channel::GrpcChannel;
#[cfg(feature = "client")]
pub use config::GrpcClientConfig;
pub use errors::{
    app_error_to_status, error_code_to_grpc_code, grpc_code_to_error_code, status_to_app_error,
};
#[cfg(feature = "client")]
pub use rskit_security::TlsConfig as GrpcClientTlsConfig;

#[cfg(all(feature = "client", feature = "discovery"))]
pub use discovery::{DiscoveryChannel, DiscoveryChannelConfig};
