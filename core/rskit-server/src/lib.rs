//! tonic gRPC server bootstrap as a lifecycle-managed Component.

#![warn(missing_docs)]

/// [`GrpcServerBuilder`] for constructing a [`GrpcServer`] component.
#[cfg(feature = "grpc")]
pub mod builder;
/// [`GrpcServer`] lifecycle component.
#[cfg(feature = "grpc")]
pub mod component;
/// [`GrpcServerConfig`] and [`TlsConfig`].
#[cfg(feature = "grpc")]
pub mod config;
/// [`ErrorLayer`] — auto-enriches gRPC error responses with structured RFC 9457 details.
#[cfg(feature = "grpc")]
pub mod error_layer;
/// HTTP server lifecycle and service endpoints.
#[cfg(feature = "http")]
pub mod http;
/// HTTP server configuration owned by `rskit-server`.
#[cfg(feature = "http")]
pub mod http_config;
/// Ordered HTTP middleware phases.
#[cfg(feature = "http")]
pub mod middleware;

#[cfg(feature = "grpc")]
pub use builder::GrpcServerBuilder;
#[cfg(feature = "grpc")]
pub use component::GrpcServer;
#[cfg(feature = "grpc")]
pub use config::{GrpcServerConfig, TlsConfig};
#[cfg(feature = "grpc")]
pub use error_layer::ErrorLayer;
#[cfg(feature = "http")]
pub use http::{HttpServer, HttpServerBuilder, observability_router};
#[cfg(feature = "http")]
pub use http_config::{CorsPolicy, HttpServerConfig};
#[cfg(feature = "http")]
pub use middleware::{
    HTTP_BASELINE_LAYER_ORDER, HTTP_INTERCEPTOR_ORDER, HttpMiddlewareStack, RouterTransform,
};
#[cfg(feature = "http")]
pub use rskit_http::SecurityHeadersLayer;
#[cfg(feature = "http")]
pub use rskit_security::{SecurityHeadersConfig, TlsConfig as HttpTlsConfig, TransportSecurity};
