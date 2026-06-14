//! API key generation, peppered digest storage, and validation helpers.

use std::fmt;

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Utc};
use hmac::{Hmac, KeyInit, Mac};
use rskit_errors::{AppError, AppResult, ErrorCode};
use serde::{Deserialize, Serialize};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

const DEFAULT_ENTROPY_BYTES: usize = 32;
const MIN_ENTROPY_BYTES: usize = 16;
const MIN_PEPPER_BYTES: usize = 32;

/// API key hashing configuration.
#[derive(Clone, Deserialize)]
pub struct HashingConfig {
    /// Secret pepper used for HMAC-SHA-256 digest storage.
    pub pepper: String,
    /// Random secret entropy in bytes.
    pub entropy_bytes: usize,
}

impl HashingConfig {
    /// Create a hashing configuration with the default entropy budget.
    #[must_use]
    pub fn new(pepper: impl Into<String>) -> Self {
        Self {
            pepper: pepper.into(),
            entropy_bytes: DEFAULT_ENTROPY_BYTES,
        }
    }

    /// Validate the configuration.
    pub fn validate(&self) -> AppResult<()> {
        if self.pepper.len() < MIN_PEPPER_BYTES {
            return Err(AppError::invalid_input(
                "pepper",
                format!("pepper must be at least {MIN_PEPPER_BYTES} bytes"),
            ));
        }
        if self.entropy_bytes < MIN_ENTROPY_BYTES {
            return Err(AppError::invalid_input(
                "entropy_bytes",
                format!("entropy_bytes must be at least {MIN_ENTROPY_BYTES}"),
            ));
        }
        Ok(())
    }
}

impl fmt::Debug for HashingConfig {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("HashingConfig")
            .field("pepper", &"<redacted>")
            .field("entropy_bytes", &self.entropy_bytes)
            .finish()
    }
}

/// Persisted API key metadata.
#[derive(Clone, Serialize, Deserialize)]
pub struct Key {
    /// Unique identifier for this key.
    pub id: String,
    /// Identifier of the key owner.
    pub owner_id: String,
    /// Human-readable name for the key.
    pub name: String,
    /// Display-safe key prefix used for candidate lookup.
    pub key_prefix: String,
    /// Pepper-keyed HMAC-SHA-256 digest stored at rest.
    pub key_digest: String,
    /// Scopes granted to the key.
    pub scopes: Vec<String>,
    /// Whether the key is active.
    pub is_active: bool,
    /// Expiry time.
    pub expires_at: Option<DateTime<Utc>>,
    /// Grace period end after rotation.
    pub grace_ends_at: Option<DateTime<Utc>>,
    /// Replacement key ID after rotation.
    pub rotated_by_id: Option<String>,
    /// Last successful validation time.
    pub last_used_at: Option<DateTime<Utc>>,
    /// Creation timestamp.
    pub created_at: DateTime<Utc>,
}

impl fmt::Debug for Key {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Key")
            .field("id", &self.id)
            .field("owner_id", &self.owner_id)
            .field("name", &self.name)
            .field("key_prefix", &self.key_prefix)
            .field("key_digest", &"<redacted>")
            .field("scopes", &self.scopes)
            .field("is_active", &self.is_active)
            .field("expires_at", &self.expires_at)
            .field("grace_ends_at", &self.grace_ends_at)
            .field("rotated_by_id", &self.rotated_by_id)
            .field("last_used_at", &self.last_used_at)
            .field("created_at", &self.created_at)
            .finish()
    }
}

impl Key {
    /// Return `true` when the key is expired and beyond any grace period.
    #[must_use]
    pub fn is_expired_past_grace(&self) -> bool {
        let now = Utc::now();
        if let Some(grace_ends_at) = self.grace_ends_at
            && now > grace_ends_at
        {
            return true;
        }
        if let Some(expires_at) = self.expires_at
            && now > expires_at
            && self.grace_ends_at.is_none()
        {
            return true;
        }
        false
    }
}

/// One-time API key material returned to callers.
#[derive(Clone, zeroize::Zeroize, zeroize::ZeroizeOnDrop)]
pub struct GenerateResult {
    /// Plaintext key shown once.
    pub plain_key: String,
    /// Prefix stored alongside metadata.
    pub key_prefix: String,
    /// Pepper-keyed digest stored at rest.
    pub key_digest: String,
}

impl fmt::Debug for GenerateResult {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("GenerateResult")
            .field("plain_key", &"<redacted>")
            .field("key_prefix", &self.key_prefix)
            .field("key_digest", &"<redacted>")
            .finish()
    }
}

/// API key hasher and issuer.
///
/// The HMAC key is validated and pre-initialized at construction time so that
/// `digest` and `compare` are fully infallible at call time.
#[derive(Clone)]
pub struct Hasher {
    config: HashingConfig,
    /// Pre-initialized HMAC state; cloned cheaply for each digest call.
    hmac_prototype: HmacSha256,
}

impl fmt::Debug for Hasher {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Hasher")
            .field("config", &self.config)
            .field("hmac_prototype", &"<redacted>")
            .finish()
    }
}

impl Hasher {
    /// Construct a new hasher, validating config and pre-initializing the HMAC key.
    pub fn new(mut config: HashingConfig) -> AppResult<Self> {
        if config.entropy_bytes == 0 {
            config.entropy_bytes = DEFAULT_ENTROPY_BYTES;
        }
        config.validate()?;
        let hmac_prototype =
            HmacSha256::new_from_slice(config.pepper.as_bytes()).map_err(|error| {
                AppError::new(
                    ErrorCode::InvalidInput,
                    format!("invalid API key pepper: {error}"),
                )
            })?;
        Ok(Self {
            config,
            hmac_prototype,
        })
    }

    /// Return the active configuration.
    #[must_use]
    pub const fn config(&self) -> &HashingConfig {
        &self.config
    }

    /// Generate a new API key with the supplied prefix.
    pub fn generate_key(&self, prefix: &str) -> AppResult<GenerateResult> {
        validate_prefix(prefix)?;

        let mut random_bytes = vec![0_u8; self.config.entropy_bytes];
        rand::fill(random_bytes.as_mut_slice());
        let secret = URL_SAFE_NO_PAD.encode(random_bytes);
        let plain_key = format!("{prefix}.{secret}");

        Ok(GenerateResult {
            key_prefix: prefix.to_string(),
            key_digest: self.digest(&plain_key),
            plain_key,
        })
    }

    /// Compute the peppered HMAC-SHA-256 digest for a plaintext key.
    ///
    /// Infallible — the HMAC key was validated at construction.
    #[must_use]
    pub fn digest(&self, plain_key: &str) -> String {
        let mut mac = self.hmac_prototype.clone();
        mac.update(plain_key.as_bytes());
        hex::encode(mac.finalize().into_bytes())
    }

    /// Constant-time digest comparison.
    #[must_use]
    pub fn compare(&self, plain_key: &str, stored_digest: &str) -> bool {
        let computed = self.digest(plain_key);
        subtle::ConstantTimeEq::ct_eq(computed.as_bytes(), stored_digest.as_bytes()).into()
    }
}

/// Split a plaintext key into `(prefix, secret)`.
pub fn split_key(plain_key: &str) -> AppResult<(String, String)> {
    let Some((prefix, secret)) = plain_key.split_once('.') else {
        return Err(AppError::invalid_input("api_key", "invalid API key format"));
    };
    if prefix.is_empty() || secret.is_empty() {
        return Err(AppError::invalid_input("api_key", "invalid API key format"));
    }
    Ok((prefix.to_string(), secret.to_string()))
}

/// Error returned when a key is not usable.
#[derive(Debug, Clone, thiserror::Error)]
#[non_exhaustive]
pub enum KeyValidationError {
    /// The key has been revoked.
    #[error("key is revoked")]
    Revoked,
    /// The key is expired past its grace period.
    #[error("key is expired")]
    Expired,
}

/// Validate key usability.
pub fn validate(key: &Key) -> Result<(), KeyValidationError> {
    if !key.is_active {
        return Err(KeyValidationError::Revoked);
    }
    if key.is_expired_past_grace() {
        return Err(KeyValidationError::Expired);
    }
    Ok(())
}

fn validate_prefix(prefix: &str) -> AppResult<()> {
    if prefix.is_empty() {
        return Err(AppError::invalid_input(
            "prefix",
            "prefix must be non-empty",
        ));
    }
    if prefix
        .chars()
        .any(|char| !char.is_ascii_alphanumeric() && char != '-' && char != '_')
    {
        return Err(AppError::invalid_input(
            "prefix",
            "prefix must contain only [A-Za-z0-9_-]",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{GenerateResult, Hasher, HashingConfig, Key, split_key};
    use chrono::Utc;

    #[test]
    fn hashing_config_debug_redacts_pepper() {
        let formatted = format!("{:?}", HashingConfig::new("very-secret-pepper-material"));
        assert!(formatted.contains("<redacted>"));
        assert!(!formatted.contains("very-secret-pepper-material"));
    }

    #[test]
    fn generate_result_debug_redacts_plain_key() {
        let formatted = format!(
            "{:?}",
            GenerateResult {
                plain_key: "sk_live.secret".into(),
                key_prefix: "sk_live".into(),
                key_digest: "digest".into(),
            }
        );
        assert!(formatted.contains("<redacted>"));
        assert!(!formatted.contains("sk_live.secret"));
    }

    #[test]
    fn key_and_hasher_debug_redact_secret_material() {
        let hasher =
            Hasher::new(HashingConfig::new("very-secret-pepper-material-32-bytes")).unwrap();
        let key = Key {
            id: "key-1".into(),
            owner_id: "owner-1".into(),
            name: "primary".into(),
            key_prefix: "pk".into(),
            key_digest: "stored-digest".into(),
            scopes: vec!["read".into()],
            is_active: true,
            expires_at: None,
            grace_ends_at: None,
            rotated_by_id: None,
            last_used_at: None,
            created_at: Utc::now(),
        };

        let hasher_debug = format!("{hasher:?}");
        assert!(hasher_debug.contains("<redacted>"));
        assert!(!hasher_debug.contains("very-secret-pepper-material-32-bytes"));

        let key_debug = format!("{key:?}");
        assert!(key_debug.contains("<redacted>"));
        assert!(!key_debug.contains("stored-digest"));
    }

    #[test]
    fn empty_api_key_prefix_is_rejected() {
        let hasher =
            Hasher::new(HashingConfig::new("very-secret-pepper-material-32-bytes")).unwrap();

        let error = hasher.generate_key("").unwrap_err();

        assert_eq!(error.code(), rskit_errors::ErrorCode::InvalidInput);
        assert!(error.message().contains("prefix"));
    }

    #[test]
    fn generate_and_compare_roundtrip() {
        let hasher = Hasher::new(HashingConfig {
            pepper: "p".repeat(32),
            entropy_bytes: 32,
        })
        .unwrap();
        let issued = hasher.generate_key("pk").unwrap();
        assert!(issued.plain_key.starts_with("pk."));
        assert!(hasher.compare(&issued.plain_key, &issued.key_digest));
    }

    #[test]
    fn split_key_rejects_malformed_values() {
        assert!(split_key("pk.secret").is_ok());
        assert!(split_key("malformed").is_err());
    }

    #[test]
    fn hashing_config_rejects_short_pepper_and_entropy() {
        let short_pepper = HashingConfig {
            pepper: "short".into(),
            entropy_bytes: 32,
        };
        assert!(short_pepper.validate().is_err());

        let short_entropy = HashingConfig {
            pepper: "p".repeat(32),
            entropy_bytes: 1,
        };
        assert!(short_entropy.validate().is_err());
    }

    #[test]
    fn key_debug_redacts_digest() {
        let key = crate::apikey::Key {
            id: "key-1".into(),
            owner_id: "user-1".into(),
            name: "primary".into(),
            key_prefix: "pk".into(),
            key_digest: "digest-secret".into(),
            scopes: Vec::new(),
            is_active: true,
            expires_at: None,
            grace_ends_at: None,
            rotated_by_id: None,
            last_used_at: None,
            created_at: chrono::Utc::now(),
        };

        let formatted = format!("{key:?}");

        assert!(formatted.contains("<redacted>"));
        assert!(!formatted.contains("digest-secret"));
    }
}
