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
/// HTTP server lifecycle and health routers.
pub mod http;
/// HTTP server configuration owned by `rskit-server`.
pub mod http_config;
/// Ordered HTTP middleware phases.
pub mod middleware;

pub use builder::GrpcServerBuilder;
pub use component::GrpcServer;
pub use config::{GrpcServerConfig, TlsConfig};
pub use error_layer::ErrorLayer;
pub use http::{HttpServer, HttpServerBuilder, health_router, healthz_router};
pub use http_config::{CorsPolicy, HttpServerConfig};
pub use middleware::{HTTP_INTERCEPTOR_ORDER, HttpMiddlewareStack, RouterTransform};
pub use rskit_http::{SecurityHeadersConfig, SecurityHeadersLayer, TransportSecurity};
