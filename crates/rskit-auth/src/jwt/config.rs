use std::{fmt, time::Duration};

use serde::Deserialize;

/// Public JWT algorithm policy for rskit.
#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
#[non_exhaustive]
pub enum JwtAlgorithm {
    /// HMAC-SHA256 for explicitly internal symmetric deployments only.
    Hs256Internal,
    /// RSA-SHA256 — preferred public-key default.
    Rs256,
    /// ECDSA P-256 / SHA-256.
    Es256,
    /// Ed25519 / `EdDSA`.
    EdDsa,
}

impl JwtAlgorithm {
    /// Return the `jsonwebtoken` algorithm for this policy entry.
    #[must_use]
    pub const fn as_jsonwebtoken(self) -> jsonwebtoken::Algorithm {
        match self {
            Self::Hs256Internal => jsonwebtoken::Algorithm::HS256,
            Self::Rs256 => jsonwebtoken::Algorithm::RS256,
            Self::Es256 => jsonwebtoken::Algorithm::ES256,
            Self::EdDsa => jsonwebtoken::Algorithm::EdDSA,
        }
    }

    /// True when the algorithm uses a symmetric shared secret.
    #[must_use]
    pub const fn is_symmetric(self) -> bool {
        matches!(self, Self::Hs256Internal)
    }
}

/// Key material used to sign and verify JWTs.
#[derive(Clone, Deserialize, zeroize::Zeroize, zeroize::ZeroizeOnDrop)]
#[non_exhaustive]
pub enum JwtKeyMaterial {
    /// Explicit internal-only HMAC secret.
    Hs256Internal {
        /// Shared secret used for signing and verification.
        secret: String,
    },
    /// RSA PEM key pair.
    Rs256 {
        /// PKCS#8 or PKCS#1 private key PEM.
        private_key_pem: String,
        /// `SubjectPublicKeyInfo` public key PEM.
        public_key_pem: String,
    },
    /// EC P-256 PEM key pair.
    Es256 {
        /// PKCS#8 private key PEM.
        private_key_pem: String,
        /// `SubjectPublicKeyInfo` public key PEM.
        public_key_pem: String,
    },
    /// Ed25519 PEM key pair.
    EdDsa {
        /// PKCS#8 private key PEM.
        private_key_pem: String,
        /// `SubjectPublicKeyInfo` public key PEM.
        public_key_pem: String,
    },
}

impl JwtKeyMaterial {
    /// Return the algorithm implied by the key material.
    #[must_use]
    pub const fn algorithm(&self) -> JwtAlgorithm {
        match self {
            Self::Hs256Internal { .. } => JwtAlgorithm::Hs256Internal,
            Self::Rs256 { .. } => JwtAlgorithm::Rs256,
            Self::Es256 { .. } => JwtAlgorithm::Es256,
            Self::EdDsa { .. } => JwtAlgorithm::EdDsa,
        }
    }
}

impl fmt::Debug for JwtKeyMaterial {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Hs256Internal { .. } => formatter
                .debug_struct("Hs256Internal")
                .field("secret", &"<redacted>")
                .finish(),
            Self::Rs256 { .. } => formatter
                .debug_struct("Rs256")
                .field("private_key_pem", &"<redacted>")
                .field("public_key_pem", &"<redacted>")
                .finish(),
            Self::Es256 { .. } => formatter
                .debug_struct("Es256")
                .field("private_key_pem", &"<redacted>")
                .field("public_key_pem", &"<redacted>")
                .finish(),
            Self::EdDsa { .. } => formatter
                .debug_struct("EdDsa")
                .field("private_key_pem", &"<redacted>")
                .field("public_key_pem", &"<redacted>")
                .finish(),
        }
    }
}

/// JWT validation policy.
#[derive(Clone, Deserialize)]
pub struct JwtConfig {
    /// Signing and verification key material.
    pub key_material: JwtKeyMaterial,
    /// Expected issuer claim.
    pub issuer: String,
    /// Accepted audience claims.
    pub audience: Vec<String>,
    /// Token time-to-live. Generation helpers may use this value.
    #[serde(default = "JwtConfig::default_ttl")]
    pub ttl: Duration,
    /// Clock-skew tolerance. Defaults to 30 seconds and must not exceed 60 seconds.
    #[serde(default = "JwtConfig::default_leeway")]
    pub leeway: Duration,
}

impl JwtConfig {
    const fn default_ttl() -> Duration {
        Duration::from_hours(1)
    }

    const fn default_leeway() -> Duration {
        Duration::from_secs(30)
    }

    /// Create an explicit internal-only HS256 configuration.
    #[must_use]
    pub fn hs256_internal(
        secret: impl Into<String>,
        issuer: impl Into<String>,
        audience: Vec<String>,
    ) -> Self {
        Self {
            key_material: JwtKeyMaterial::Hs256Internal {
                secret: secret.into(),
            },
            issuer: issuer.into(),
            audience,
            ttl: Self::default_ttl(),
            leeway: Self::default_leeway(),
        }
    }

    /// Create an RS256 configuration from PEM-encoded keys.
    #[must_use]
    pub fn rs256(
        private_key_pem: impl Into<String>,
        public_key_pem: impl Into<String>,
        issuer: impl Into<String>,
        audience: Vec<String>,
    ) -> Self {
        Self {
            key_material: JwtKeyMaterial::Rs256 {
                private_key_pem: private_key_pem.into(),
                public_key_pem: public_key_pem.into(),
            },
            issuer: issuer.into(),
            audience,
            ttl: Self::default_ttl(),
            leeway: Self::default_leeway(),
        }
    }

    /// Create an ES256 configuration from PEM-encoded keys.
    #[must_use]
    pub fn es256(
        private_key_pem: impl Into<String>,
        public_key_pem: impl Into<String>,
        issuer: impl Into<String>,
        audience: Vec<String>,
    ) -> Self {
        Self {
            key_material: JwtKeyMaterial::Es256 {
                private_key_pem: private_key_pem.into(),
                public_key_pem: public_key_pem.into(),
            },
            issuer: issuer.into(),
            audience,
            ttl: Self::default_ttl(),
            leeway: Self::default_leeway(),
        }
    }

    /// Create an `EdDSA` configuration from PEM-encoded keys.
    #[must_use]
    pub fn eddsa(
        private_key_pem: impl Into<String>,
        public_key_pem: impl Into<String>,
        issuer: impl Into<String>,
        audience: Vec<String>,
    ) -> Self {
        Self {
            key_material: JwtKeyMaterial::EdDsa {
                private_key_pem: private_key_pem.into(),
                public_key_pem: public_key_pem.into(),
            },
            issuer: issuer.into(),
            audience,
            ttl: Self::default_ttl(),
            leeway: Self::default_leeway(),
        }
    }

    /// Override the configured token TTL.
    #[must_use]
    pub const fn with_ttl(mut self, ttl: Duration) -> Self {
        self.ttl = ttl;
        self
    }

    /// Override clock skew tolerance.
    #[must_use]
    pub const fn with_leeway(mut self, leeway: Duration) -> Self {
        self.leeway = leeway;
        self
    }

    /// Return the effective JWT algorithm.
    #[must_use]
    pub const fn algorithm(&self) -> JwtAlgorithm {
        self.key_material.algorithm()
    }
}

impl fmt::Debug for JwtConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("JwtConfig")
            .field("key_material", &self.key_material)
            .field("issuer", &self.issuer)
            .field("audience", &self.audience)
            .field("ttl", &self.ttl)
            .field("leeway", &self.leeway)
            .finish()
    }
}

#[cfg(test)]
mod tests {
    use super::{JwtConfig, JwtKeyMaterial};

    #[test]
    fn jwt_key_material_debug_redacts_secret_values() {
        let symmetric = format!(
            "{:?}",
            JwtKeyMaterial::Hs256Internal {
                secret: "super-secret-value".into(),
            }
        );
        assert!(symmetric.contains("<redacted>"));
        assert!(!symmetric.contains("super-secret-value"));

        let asymmetric = format!(
            "{:?}",
            JwtKeyMaterial::Rs256 {
                private_key_pem: "private-pem".into(),
                public_key_pem: "public-pem".into(),
            }
        );
        assert!(asymmetric.contains("<redacted>"));
        assert!(!asymmetric.contains("private-pem"));
        assert!(!asymmetric.contains("public-pem"));
    }

    #[test]
    fn jwt_config_debug_redacts_nested_key_material() {
        let config = JwtConfig::hs256_internal(
            "another-secret-value",
            "issuer.example",
            vec!["audience".into()],
        );

        let formatted = format!("{config:?}");

        assert!(formatted.contains("<redacted>"));
        assert!(!formatted.contains("another-secret-value"));
        assert!(formatted.contains("issuer.example"));
    }
}
