//! Shared security configuration used across transports.
//!
//! This crate owns lower-level security vocabulary that multiple transport and
//! auth crates need without depending on each other: TLS policy,
//! [`SecretString`], secure HTTP response headers, and shared HTTP auth scheme
//! constants.

#![warn(missing_docs)]

/// HTTP security header policy and shared auth scheme constants.
pub mod http;
/// Secret redaction and constant-time comparison helpers.
pub mod secret;
/// TLS material and verification policy.
pub mod tls;

pub use http::{BASIC_AUTH_SCHEME, BEARER_AUTH_SCHEME, SecurityHeadersConfig, TransportSecurity};
pub use secret::{SecretString, constant_time_eq};
pub use tls::{TlsConfig, TlsVersion};
