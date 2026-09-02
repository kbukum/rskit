use std::{fmt, time::Duration};

use rskit_util::SecretString;
use serde::{Deserialize, Serialize};

/// Public JWT algorithm policy for rskit.
///
/// The allow-list uses canonical JWA spellings (`HS256`, `RS256`, `ES256`, `EdDSA`).
/// `none` and any unlisted algorithm cannot be represented and are rejected during deserialization.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[non_exhaustive]
pub enum JwtAlgorithm {
    /// HMAC-SHA256. Symmetric; permitted only when `allow_symmetric_hmac` is enabled.
    #[serde(rename = "HS256")]
    Hs256,
    /// RSA-SHA256 — preferred public-key default.
    #[serde(rename = "RS256")]
    Rs256,
    /// ECDSA P-256 / SHA-256.
    #[serde(rename = "ES256")]
    Es256,
    /// Ed25519 / `EdDSA`.
    #[serde(rename = "EdDSA")]
    EdDsa,
}

impl JwtAlgorithm {
    /// Return the `jsonwebtoken` algorithm for this policy entry.
    #[must_use]
    pub const fn as_jsonwebtoken(self) -> jsonwebtoken::Algorithm {
        match self {
            Self::Hs256 => jsonwebtoken::Algorithm::HS256,
            Self::Rs256 => jsonwebtoken::Algorithm::RS256,
            Self::Es256 => jsonwebtoken::Algorithm::ES256,
            Self::EdDsa => jsonwebtoken::Algorithm::EdDSA,
        }
    }

    /// True when the algorithm uses a symmetric shared secret.
    #[must_use]
    pub const fn is_symmetric(self) -> bool {
        matches!(self, Self::Hs256)
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
    #[serde(rename = "EdDSA")]
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
    /// Symmetric HMAC secret. Permitted only when `allow_symmetric_hmac` is enabled.
    Hmac {
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
            Self::Hmac { .. } => JwtAlgorithm::Hs256,
            Self::Asymmetric { algorithm, .. } => algorithm.as_jwt_algorithm(),
        }
    }
}

impl fmt::Debug for JwtKeyMaterial {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Hmac { secret } => f.debug_struct("Hmac").field("secret", secret).finish(),
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
    /// Access-token time-to-live. Advisory: callers own claim construction and set `exp` themselves.
    ///
    /// Deserializes from the shared `access_token_ttl` key.
    #[serde(rename = "access_token_ttl", default = "JwtConfig::default_ttl")]
    pub ttl: Duration,
    /// Refresh-token time-to-live. Advisory: neither [`crate::JwtService::generate_refresh`] nor
    /// [`crate::JwtCodec::encode_refresh`] reads this value or sets `exp` — callers building refresh
    /// claims size the expiry themselves.
    ///
    /// Deserializes from the shared `refresh_token_ttl` key.
    #[serde(
        rename = "refresh_token_ttl",
        default = "JwtConfig::default_refresh_ttl"
    )]
    pub refresh_ttl: Duration,
    /// Clock-skew tolerance. Defaults to 30 seconds and must not exceed 60 seconds.
    ///
    /// Deserializes from the shared `clock_skew` key.
    #[serde(rename = "clock_skew", default = "JwtConfig::default_leeway")]
    pub leeway: Duration,
    /// Explicit opt-in required to use the symmetric HMAC (`HS256`) algorithm.
    ///
    /// HMAC signing is intended for internal-only deployments; asymmetric keys are preferred.
    #[serde(default)]
    pub allow_symmetric_hmac: bool,
    /// Optional separate secret for refresh tokens (HMAC only). When unset, the primary
    /// key material signs and verifies both access and refresh tokens.
    #[serde(default)]
    pub refresh_secret: Option<SecretString>,
}

impl JwtConfig {
    const fn default_ttl() -> Duration {
        Duration::from_hours(1)
    }

    const fn default_refresh_ttl() -> Duration {
        Duration::from_hours(24 * 7)
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
            refresh_ttl: Self::default_refresh_ttl(),
            leeway: Self::default_leeway(),
            allow_symmetric_hmac: false,
            refresh_secret: None,
        }
    }

    /// Create a symmetric HMAC (`HS256`) configuration.
    ///
    /// Constructing an HMAC configuration explicitly opts into symmetric signing
    /// (`allow_symmetric_hmac`), which is intended for internal-only deployments.
    #[must_use]
    pub fn hmac(
        secret: impl Into<String>,
        issuer: impl Into<String>,
        audience: Vec<String>,
    ) -> Self {
        Self {
            key_material: JwtKeyMaterial::Hmac {
                secret: SecretString::new(secret),
            },
            issuer: issuer.into(),
            audience,
            ttl: Self::default_ttl(),
            refresh_ttl: Self::default_refresh_ttl(),
            leeway: Self::default_leeway(),
            allow_symmetric_hmac: true,
            refresh_secret: None,
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
        Self::asymmetric(
            AsymmetricAlgorithm::Rs256,
            private_key_pem,
            public_key_pem,
            issuer,
            audience,
        )
    }

    /// Create an ES256 configuration from PEM-encoded keys.
    #[must_use]
    pub fn es256(
        private_key_pem: impl Into<String>,
        public_key_pem: impl Into<String>,
        issuer: impl Into<String>,
        audience: Vec<String>,
    ) -> Self {
        Self::asymmetric(
            AsymmetricAlgorithm::Es256,
            private_key_pem,
            public_key_pem,
            issuer,
            audience,
        )
    }

    /// Create an `EdDSA` configuration from PEM-encoded keys.
    #[must_use]
    pub fn eddsa(
        private_key_pem: impl Into<String>,
        public_key_pem: impl Into<String>,
        issuer: impl Into<String>,
        audience: Vec<String>,
    ) -> Self {
        Self::asymmetric(
            AsymmetricAlgorithm::EdDsa,
            private_key_pem,
            public_key_pem,
            issuer,
            audience,
        )
    }

    /// Override the configured access-token TTL.
    #[must_use]
    pub const fn with_ttl(mut self, ttl: Duration) -> Self {
        self.ttl = ttl;
        self
    }

    /// Override the configured refresh-token TTL.
    #[must_use]
    pub const fn with_refresh_ttl(mut self, refresh_ttl: Duration) -> Self {
        self.refresh_ttl = refresh_ttl;
        self
    }

    /// Override clock skew tolerance.
    #[must_use]
    pub const fn with_leeway(mut self, leeway: Duration) -> Self {
        self.leeway = leeway;
        self
    }

    /// Set a separate HMAC secret for refresh tokens.
    #[must_use]
    pub fn with_refresh_secret(mut self, refresh_secret: impl Into<String>) -> Self {
        self.refresh_secret = Some(SecretString::new(refresh_secret));
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
            .field("refresh_ttl", &self.refresh_ttl)
            .field("leeway", &self.leeway)
            .field("allow_symmetric_hmac", &self.allow_symmetric_hmac)
            .field("refresh_secret", &self.refresh_secret)
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
            JwtKeyMaterial::Hmac {
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
    fn jwt_algorithm_policy_identifies_symmetric_hmac_mode() {
        assert!(JwtAlgorithm::Hs256.is_symmetric());
        assert!(!JwtAlgorithm::Rs256.is_symmetric());
        assert!(!JwtAlgorithm::Es256.is_symmetric());
        assert!(!JwtAlgorithm::EdDsa.is_symmetric());
    }

    #[test]
    fn jwt_algorithm_serde_uses_jwa_spellings() {
        for (algorithm, wire) in [
            (JwtAlgorithm::Hs256, "\"HS256\""),
            (JwtAlgorithm::Rs256, "\"RS256\""),
            (JwtAlgorithm::Es256, "\"ES256\""),
            (JwtAlgorithm::EdDsa, "\"EdDSA\""),
        ] {
            assert_eq!(serde_json::to_string(&algorithm).unwrap(), wire);
            assert_eq!(
                serde_json::from_str::<JwtAlgorithm>(wire).unwrap(),
                algorithm
            );
        }
    }

    #[test]
    fn jwt_algorithm_rejects_none_and_legacy_internal_spellings() {
        for wire in ["\"none\"", "\"HS256_INTERNAL\"", "\"EDDSA\"", "\"HS512\""] {
            assert!(serde_json::from_str::<JwtAlgorithm>(wire).is_err());
        }
    }

    #[test]
    fn jwt_config_builders_preserve_ttl_and_leeway_overrides() {
        let config = JwtConfig::hmac(
            "secret-material-that-is-long-enough",
            "https://issuer.example",
            vec!["audience".into()],
        )
        .with_ttl(Duration::from_mins(5))
        .with_refresh_ttl(Duration::from_hours(48))
        .with_leeway(Duration::from_secs(10));

        assert_eq!(config.ttl, Duration::from_mins(5));
        assert_eq!(config.refresh_ttl, Duration::from_hours(48));
        assert_eq!(config.leeway, Duration::from_secs(10));
        assert!(config.allow_symmetric_hmac);
    }

    #[test]
    fn jwt_config_debug_redacts_nested_key_material() {
        let config = JwtConfig::hmac(
            "another-secret-value",
            "issuer.example",
            vec!["audience".into()],
        )
        .with_refresh_secret("refresh-secret-value-that-is-long");

        let formatted = format!("{config:?}");

        assert!(formatted.contains("***"));
        assert!(!formatted.contains("another-secret-value"));
        assert!(!formatted.contains("refresh-secret-value-that-is-long"));
        assert!(formatted.contains("issuer.example"));
    }

    #[test]
    fn convenience_constructors_delegate_to_asymmetric() {
        let rs = JwtConfig::rs256("priv", "pub", "iss", vec!["aud".into()]);
        assert_eq!(rs.algorithm(), JwtAlgorithm::Rs256);
        assert!(!rs.allow_symmetric_hmac);

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

        let json = r#""EdDSA""#;
        let alg: AsymmetricAlgorithm = serde_json::from_str(json).unwrap();
        assert_eq!(alg, AsymmetricAlgorithm::EdDsa);
    }

    #[test]
    fn config_deserializes_shared_keys() {
        let shared = r#"{
            "key_material": {"Hmac": {"secret": "my-secret"}},
            "issuer": "iss",
            "audience": ["aud"],
            "access_token_ttl": {"secs": 900, "nanos": 0},
            "refresh_token_ttl": {"secs": 604800, "nanos": 0},
            "clock_skew": {"secs": 15, "nanos": 0},
            "allow_symmetric_hmac": true
        }"#;
        let cfg: JwtConfig = serde_json::from_str(shared).unwrap();
        assert_eq!(cfg.ttl, Duration::from_mins(15));
        assert_eq!(cfg.refresh_ttl, Duration::from_hours(168));
        assert_eq!(cfg.leeway, Duration::from_secs(15));
        assert!(cfg.allow_symmetric_hmac);
    }

    #[test]
    fn key_material_serde_symmetric() {
        let json = r#"{"Hmac": {"secret": "my-secret"}}"#;
        let mat: JwtKeyMaterial = serde_json::from_str(json).unwrap();
        assert_eq!(mat.algorithm(), JwtAlgorithm::Hs256);
    }

    #[test]
    fn key_material_serde_asymmetric() {
        let json = r#"{"Asymmetric": {"algorithm": "RS256", "private_key_pem": "priv", "public_key_pem": "pub"}}"#;
        let mat: JwtKeyMaterial = serde_json::from_str(json).unwrap();
        assert_eq!(mat.algorithm(), JwtAlgorithm::Rs256);
    }
}
