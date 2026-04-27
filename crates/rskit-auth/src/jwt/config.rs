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

    /// Create a new config with just a secret (all other fields default).
    #[must_use]
    pub fn new(secret: impl Into<String>) -> Self {
        Self {
            secret: secret.into(),
            algorithm: Algorithm::HS256,
            ttl: Self::default_ttl(),
            issuer: None,
            audience: None,
        }
    }

    /// Set the signing algorithm.
    #[must_use]
    pub const fn with_algorithm(mut self, algorithm: Algorithm) -> Self {
        self.algorithm = algorithm;
        self
    }

    /// Set the token TTL.
    #[must_use]
    pub const fn with_ttl(mut self, ttl: Duration) -> Self {
        self.ttl = ttl;
        self
    }

    /// Set the expected issuer claim.
    #[must_use]
    pub fn with_issuer(mut self, issuer: impl Into<String>) -> Self {
        self.issuer = Some(issuer.into());
        self
    }

    /// Set the expected audience claims.
    #[must_use]
    pub fn with_audience(mut self, audience: Vec<String>) -> Self {
        self.audience = Some(audience);
        self
    }
}
