//! Shared security configuration used across transports.

#![warn(missing_docs)]

/// HTTP security header policy.
pub mod http;
/// TLS material and verification policy.
pub mod tls;

pub use http::{SecurityHeadersConfig, TransportSecurity};
pub use tls::{TlsConfig, TlsVersion};
