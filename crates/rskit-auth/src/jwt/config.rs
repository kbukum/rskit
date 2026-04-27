use std::time::Duration;

use serde::Deserialize;

/// JWT signing algorithm.
#[derive(Debug, Clone, Deserialize, Default)]
#[serde(rename_all = "UPPERCASE")]
#[non_exhaustive]
pub enum Algorithm {
    /// HMAC-SHA256 (default).
    #[default]
    HS256,
    /// HMAC-SHA384.
    HS384,
    /// HMAC-SHA512.
    HS512,
    /// RSA-SHA256.
    RS256,
    /// RSA-SHA384.
    RS384,
    /// RSA-SHA512.
    RS512,
}

impl From<&Algorithm> for jsonwebtoken::Algorithm {
    fn from(a: &Algorithm) -> Self {
        match a {
            Algorithm::HS256 => Self::HS256,
            Algorithm::HS384 => Self::HS384,
            Algorithm::HS512 => Self::HS512,
            Algorithm::RS256 => Self::RS256,
            Algorithm::RS384 => Self::RS384,
            Algorithm::RS512 => Self::RS512,
        }
    }
}

/// JWT configuration.
#[derive(Debug, Clone, Deserialize, zeroize::Zeroize, zeroize::ZeroizeOnDrop)]
pub struct JwtConfig {
    /// HMAC secret or RSA private-key PEM.
    pub secret: String,
    /// Signing algorithm (default: HS256).
    #[zeroize(skip)]
    #[serde(default)]
    pub algorithm: Algorithm,
    /// Token time-to-live (default: 1 hour).
    #[zeroize(skip)]
    #[serde(default = "JwtConfig::default_ttl")]
    pub ttl: Duration,
    /// Expected issuer claim (`iss`).
    pub issuer: Option<String>,
    /// Expected audience claims (`aud`).
    pub audience: Option<Vec<String>>,
}

impl Default for JwtConfig {
    fn default() -> Self {
        Self {
            secret: String::new(),
            algorithm: Algorithm::HS256,
            ttl: Self::default_ttl(),
            issuer: None,
            audience: None,
        }
    }
}

impl JwtConfig {
    const fn default_ttl() -> Duration {
        Duration::from_secs(3600)
    }
}
