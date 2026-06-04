//! Shared security configuration used across transports.

#![warn(missing_docs)]

/// HTTP security header policy.
pub mod http;
/// Secret redaction and constant-time comparison helpers.
pub mod secret;
/// TLS material and verification policy.
pub mod tls;

pub use http::{SecurityHeadersConfig, TransportSecurity};
pub use secret::{SecretString, constant_time_eq};
pub use tls::{TlsConfig, TlsVersion};
