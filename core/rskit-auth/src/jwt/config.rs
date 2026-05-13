use std::{fmt, time::Duration};

use rskit_util::SecretString;
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

/// Asymmetric signing algorithm.
#[derive(Clone, Copy, PartialEq, Eq, Deserialize)]
#[non_exhaustive]
pub enum AsymmetricAlgorithm {
    /// RSA-SHA256.
    #[serde(rename = "RS256")]
    Rs256,
    /// ECDSA P-256 / SHA-256.
    #[serde(rename = "ES256")]
    Es256,
    /// Ed25519 / `EdDSA`.
    #[serde(rename = "EDDSA")]
    EdDsa,
}

impl AsymmetricAlgorithm {
    /// Map to the public-facing [`JwtAlgorithm`].
    #[must_use]
    pub const fn as_jwt_algorithm(self) -> JwtAlgorithm {
        match self {
            Self::Rs256 => JwtAlgorithm::Rs256,
            Self::Es256 => JwtAlgorithm::Es256,
            Self::EdDsa => JwtAlgorithm::EdDsa,
        }
    }
}

impl fmt::Debug for AsymmetricAlgorithm {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Rs256 => f.write_str("RS256"),
            Self::Es256 => f.write_str("ES256"),
            Self::EdDsa => f.write_str("EdDSA"),
        }
    }
}

// Algorithm identifier is not secret — no-op zeroize.
impl zeroize::Zeroize for AsymmetricAlgorithm {
    fn zeroize(&mut self) {}
}

/// PEM key pair for asymmetric algorithms.
#[derive(Clone, Deserialize, zeroize::Zeroize, zeroize::ZeroizeOnDrop)]
pub struct KeyPair {
    /// PKCS#8 (or PKCS#1 for RSA) private key PEM.
    pub private_key_pem: SecretString,
    /// `SubjectPublicKeyInfo` public key PEM.
    pub public_key_pem: SecretString,
}

impl fmt::Debug for KeyPair {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("KeyPair")
            .field("private_key_pem", &self.private_key_pem)
            .field("public_key_pem", &self.public_key_pem)
            .finish()
    }
}

/// Key material used to sign and verify JWTs.
#[derive(Clone, Deserialize, zeroize::Zeroize, zeroize::ZeroizeOnDrop)]
#[non_exhaustive]
pub enum JwtKeyMaterial {
    /// Explicit internal-only HMAC secret.
    Hs256Internal {
        /// Shared secret used for signing and verification.
        secret: SecretString,
    },
    /// Asymmetric PEM key pair (RS256, ES256, or `EdDSA`).
    Asymmetric {
        /// Which asymmetric algorithm this key pair is for.
        algorithm: AsymmetricAlgorithm,
        /// The PEM key pair.
        #[serde(flatten)]
        keys: KeyPair,
    },
}

impl JwtKeyMaterial {
    /// Return the algorithm implied by the key material.
    #[must_use]
    pub const fn algorithm(&self) -> JwtAlgorithm {
        match self {
            Self::Hs256Internal { .. } => JwtAlgorithm::Hs256Internal,
            Self::Asymmetric { algorithm, .. } => algorithm.as_jwt_algorithm(),
        }
    }
}

impl fmt::Debug for JwtKeyMaterial {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Hs256Internal { secret } => f
                .debug_struct("Hs256Internal")
                .field("secret", secret)
                .finish(),
            Self::Asymmetric { algorithm, keys } => f
                .debug_struct("Asymmetric")
                .field("algorithm", algorithm)
                .field("keys", keys)
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

    /// Create an asymmetric key configuration.
    #[must_use]
    pub fn asymmetric(
        algorithm: AsymmetricAlgorithm,
        private_key_pem: impl Into<String>,
        public_key_pem: impl Into<String>,
        issuer: impl Into<String>,
        audience: Vec<String>,
    ) -> Self {
        Self {
            key_material: JwtKeyMaterial::Asymmetric {
                algorithm,
                keys: KeyPair {
                    private_key_pem: SecretString::new(private_key_pem),
                    public_key_pem: SecretString::new(public_key_pem),
                },
            },
            issuer: issuer.into(),
            audience,
            ttl: Self::default_ttl(),
            leeway: Self::default_leeway(),
        }
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
                secret: SecretString::new(secret),
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
        Self::asymmetric(AsymmetricAlgorithm::Rs256, private_key_pem, public_key_pem, issuer, audience)
    }

    /// Create an ES256 configuration from PEM-encoded keys.
    #[must_use]
    pub fn es256(
        private_key_pem: impl Into<String>,
        public_key_pem: impl Into<String>,
        issuer: impl Into<String>,
        audience: Vec<String>,
    ) -> Self {
        Self::asymmetric(AsymmetricAlgorithm::Es256, private_key_pem, public_key_pem, issuer, audience)
    }

    /// Create an `EdDSA` configuration from PEM-encoded keys.
    #[must_use]
    pub fn eddsa(
        private_key_pem: impl Into<String>,
        public_key_pem: impl Into<String>,
        issuer: impl Into<String>,
        audience: Vec<String>,
    ) -> Self {
        Self::asymmetric(AsymmetricAlgorithm::EdDsa, private_key_pem, public_key_pem, issuer, audience)
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
    use super::*;

    #[test]
    fn jwt_key_material_debug_redacts_secret_values() {
        let symmetric = format!(
            "{:?}",
            JwtKeyMaterial::Hs256Internal {
                secret: SecretString::new("super-secret-value"),
            }
        );
        assert!(symmetric.contains("***"));
        assert!(!symmetric.contains("super-secret-value"));

        let asymmetric = format!(
            "{:?}",
            JwtKeyMaterial::Asymmetric {
                algorithm: AsymmetricAlgorithm::Rs256,
                keys: KeyPair {
                    private_key_pem: SecretString::new("private-pem"),
                    public_key_pem: SecretString::new("public-pem"),
                },
            }
        );
        assert!(asymmetric.contains("***"));
        assert!(asymmetric.contains("RS256"));
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

        assert!(formatted.contains("***"));
        assert!(!formatted.contains("another-secret-value"));
        assert!(formatted.contains("issuer.example"));
    }

    #[test]
    fn convenience_constructors_delegate_to_asymmetric() {
        let rs = JwtConfig::rs256("priv", "pub", "iss", vec!["aud".into()]);
        assert_eq!(rs.algorithm(), JwtAlgorithm::Rs256);

        let es = JwtConfig::es256("priv", "pub", "iss", vec!["aud".into()]);
        assert_eq!(es.algorithm(), JwtAlgorithm::Es256);

        let ed = JwtConfig::eddsa("priv", "pub", "iss", vec!["aud".into()]);
        assert_eq!(ed.algorithm(), JwtAlgorithm::EdDsa);
    }

    #[test]
    fn asymmetric_algorithm_roundtrip_serde() {
        let json = r#""RS256""#;
        let alg: AsymmetricAlgorithm = serde_json::from_str(json).unwrap();
        assert_eq!(alg, AsymmetricAlgorithm::Rs256);

        let json = r#""EDDSA""#;
        let alg: AsymmetricAlgorithm = serde_json::from_str(json).unwrap();
        assert_eq!(alg, AsymmetricAlgorithm::EdDsa);
    }

    #[test]
    fn key_material_serde_symmetric() {
        let json = r#"{"Hs256Internal": {"secret": "my-secret"}}"#;
        let mat: JwtKeyMaterial = serde_json::from_str(json).unwrap();
        assert_eq!(mat.algorithm(), JwtAlgorithm::Hs256Internal);
    }

    #[test]
    fn key_material_serde_asymmetric() {
        let json = r#"{"Asymmetric": {"algorithm": "RS256", "private_key_pem": "priv", "public_key_pem": "pub"}}"#;
        let mat: JwtKeyMaterial = serde_json::from_str(json).unwrap();
        assert_eq!(mat.algorithm(), JwtAlgorithm::Rs256);
    }
}
