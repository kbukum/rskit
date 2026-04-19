//! tonic gRPC server bootstrap as a lifecycle-managed Component.

#![warn(missing_docs)]

/// [`GrpcServerBuilder`] for constructing a [`GrpcServer`] component.
pub mod builder;
/// [`GrpcServer`] lifecycle component.
pub mod component;
/// [`GrpcServerConfig`] and [`TlsConfig`].
pub mod config;
/// [`ErrorLayer`] — auto-enriches gRPC error responses with structured RFC 9457 details.
pub mod error_layer;

pub use builder::GrpcServerBuilder;
pub use component::GrpcServer;
pub use config::{GrpcServerConfig, TlsConfig};
pub use error_layer::ErrorLayer;
