#![warn(missing_docs)]
#![doc = include_str!("../README.md")]

//! gRPC transport entrypoints for rskit.
//!
//! `rskit-grpc` owns client-side gRPC transport concerns: lazy channels,
//! TLS-aware dialing, error mapping, and optional discovery integration.
//!
//! Lifecycle-managed gRPC servers are owned by `rskit-server`.

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
pub use config::GrpcClientConfig;
#[cfg(feature = "client")]
pub use errors::{app_error_to_status, status_to_app_error};
#[cfg(feature = "client")]
pub use rskit_security::TlsConfig as GrpcClientTlsConfig;

#[cfg(all(feature = "client", feature = "discovery"))]
pub use discovery::{DiscoveryChannel, DiscoveryChannelConfig};
