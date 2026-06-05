//! JWT sign and verify using [`jsonwebtoken`].

mod codec;
mod config;
mod service;

pub use codec::{JwtCodec, JwtHeader};
pub use config::{AsymmetricAlgorithm, JwtAlgorithm, JwtConfig, JwtKeyMaterial, KeyPair};
pub use service::JwtService;
