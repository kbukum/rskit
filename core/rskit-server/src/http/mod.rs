//! HTTP server component: builder, lifecycle, connection serving, TLS, and health routers.

mod builder;
mod component;
mod health;
mod serve;
mod tls;

#[cfg(test)]
mod test_support;

pub use builder::HttpServerBuilder;
pub use component::HttpServer;
pub use health::{health_router, healthz_router};
