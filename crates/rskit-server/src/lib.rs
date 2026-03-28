//! tonic gRPC server bootstrap as a lifecycle-managed Component.

#![warn(missing_docs)]

/// [`GrpcServerBuilder`] for constructing a [`GrpcServer`] component.
pub mod builder;
/// [`GrpcServer`] lifecycle component.
pub mod component;
/// [`GrpcServerConfig`] and [`TlsConfig`].
pub mod config;

pub use builder::GrpcServerBuilder;
pub use component::GrpcServer;
pub use config::{GrpcServerConfig, TlsConfig};
