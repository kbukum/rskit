//! HTTP security policy and auth vocabulary shared by transport adapters.

mod auth;
mod headers;

pub use auth::{BASIC_AUTH_SCHEME, BEARER_AUTH_SCHEME};
pub use headers::{SecurityHeadersConfig, TransportSecurity};
