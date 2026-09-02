//! HTTP server component: builder, lifecycle, connection serving, TLS, and service endpoints.

mod builder;
mod component;
mod endpoints;
mod serve;
mod tls;

#[cfg(test)]
mod test_support;

pub use builder::HttpServerBuilder;
pub use component::HttpServer;
pub use endpoints::observability_router;
