#![allow(
    missing_docs,
    clippy::derive_partial_eq_without_eq,
    clippy::redundant_pub_crate
)]

use std::time::{SystemTime, UNIX_EPOCH};

use rskit_auth::{JwtConfig, JwtService};
use serde::{Deserialize, Serialize};

pub(crate) const ISSUER: &str = "https://issuer.rskit.test";
pub(crate) const AUDIENCE: &str = "rskit-auth-tests";

pub(crate) fn now_epoch() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

pub(crate) fn future_exp() -> u64 {
    now_epoch() + 3600
}

pub(crate) fn standard_config(secret: &str) -> JwtConfig {
    // Pad short secrets to the 32-byte minimum required by the HMAC key validator.
    let padded;
    let key = if secret.len() < 32 {
        padded = format!("{secret:-<32}");
        padded.as_str()
    } else {
        secret
    };
    JwtConfig::hmac(key, ISSUER, vec![AUDIENCE.to_string()])
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub(crate) struct StandardClaims {
    pub(crate) sub: String,
    pub(crate) iss: String,
    pub(crate) aud: Vec<String>,
    pub(crate) exp: u64,
    pub(crate) nbf: u64,
    pub(crate) iat: u64,
}

impl StandardClaims {
    pub(crate) fn new(sub: impl Into<String>) -> Self {
        let now = now_epoch();
        Self {
            sub: sub.into(),
            iss: ISSUER.into(),
            aud: vec![AUDIENCE.into()],
            exp: future_exp(),
            nbf: now.saturating_sub(1),
            iat: now,
        }
    }
}

pub(crate) fn jwt_service(secret: &str) -> JwtService<StandardClaims> {
    JwtService::new(standard_config(secret)).unwrap()
}
