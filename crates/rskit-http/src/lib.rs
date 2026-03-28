//! Axum HTTP server with graceful shutdown, CORS, request-ID, and health endpoint.

#![warn(missing_docs)]

mod config;
mod error;
mod extractors;
mod server;

pub use config::{CorsConfig, HttpServerConfig};
pub use error::{ErrorHandlerLayer, HttpError};
pub use extractors::{CorrelationId, RequestId};
pub use server::{HttpServer, HttpServerBuilder, health_router};
