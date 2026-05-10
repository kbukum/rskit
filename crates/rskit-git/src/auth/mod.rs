//! Authentication and signing support types.

mod provider;
mod signing;
mod transport;

pub use provider::AuthProvider;
pub use signing::SigningConfig;
pub use transport::TransportAuth;
