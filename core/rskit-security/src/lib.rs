//! Shared security configuration used across transports.

#![warn(missing_docs)]

/// TLS material and verification policy.
pub mod tls;

pub use tls::{TlsConfig, TlsVersion};
