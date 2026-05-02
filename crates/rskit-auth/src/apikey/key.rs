//! API key generation, peppered digest storage, and validation helpers.

use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
use chrono::{DateTime, Utc};
use hmac::{Hmac, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;

type HmacSha256 = Hmac<Sha256>;

const DEFAULT_ENTROPY_BYTES: usize = 32;
const MIN_ENTROPY_BYTES: usize = 16;
const MIN_PEPPER_BYTES: usize = 32;

/// API key hashing configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HashingConfig {
    /// Secret pepper used for HMAC-SHA-256 digest storage.
    pub pepper: String,
    /// Random secret entropy in bytes.
    pub entropy_bytes: usize,
}

impl Default for HashingConfig {
    fn default() -> Self {
        Self {
            pepper: String::new(),
            entropy_bytes: DEFAULT_ENTROPY_BYTES,
        }
    }
}

impl HashingConfig {
    /// Validate the configuration.
    pub fn validate(&self) -> Result<(), String> {
        if self.pepper.len() < MIN_PEPPER_BYTES {
            return Err(format!(
                "apikey: pepper must be at least {MIN_PEPPER_BYTES} bytes"
            ));
        }
        if self.entropy_bytes < MIN_ENTROPY_BYTES {
            return Err(format!(
                "apikey: entropy_bytes must be at least {MIN_ENTROPY_BYTES}"
            ));
        }
        Ok(())
    }
}

/// Persisted API key metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, zeroize::Zeroize, zeroize::ZeroizeOnDrop)]
pub struct GenerateResult {
    /// Plaintext key shown once.
    pub plain_key: String,
    /// Prefix stored alongside metadata.
    pub key_prefix: String,
    /// Pepper-keyed digest stored at rest.
    pub key_digest: String,
}

/// API key hasher and issuer.
#[derive(Debug, Clone)]
pub struct Hasher {
    config: HashingConfig,
}

impl Hasher {
    /// Construct a new hasher.
    pub fn new(mut config: HashingConfig) -> Result<Self, String> {
        if config.entropy_bytes == 0 {
            config.entropy_bytes = DEFAULT_ENTROPY_BYTES;
        }
        config.validate()?;
        Ok(Self { config })
    }

    /// Return the active configuration.
    #[must_use]
    pub const fn config(&self) -> &HashingConfig {
        &self.config
    }

    /// Generate a new API key with the supplied prefix.
    pub fn generate_key(&self, prefix: &str) -> Result<GenerateResult, String> {
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

    /// Compute the peppered digest for a plaintext key.
    #[must_use]
    pub fn digest(&self, plain_key: &str) -> String {
        let mut mac = HmacSha256::new_from_slice(self.config.pepper.as_bytes())
            .expect("pepper length validated");
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
pub fn split_key(plain_key: &str) -> Result<(String, String), String> {
    let Some((prefix, secret)) = plain_key.split_once('.') else {
        return Err(String::from("apikey: invalid key format"));
    };
    if prefix.is_empty() || secret.is_empty() {
        return Err(String::from("apikey: invalid key format"));
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

fn validate_prefix(prefix: &str) -> Result<(), String> {
    if prefix.is_empty() {
        return Err(String::from("apikey: prefix must be non-empty"));
    }
    if prefix
        .chars()
        .any(|char| !char.is_ascii_alphanumeric() && char != '-' && char != '_')
    {
        return Err(String::from(
            "apikey: prefix must contain only [A-Za-z0-9_-]",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{Hasher, HashingConfig, split_key};

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
}
