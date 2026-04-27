//! API key data structure, generation, hashing, and validation.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// API key metadata (never stores the plaintext).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Key {
    /// Unique identifier for this key.
    pub id: String,
    /// Identifier of the key owner.
    pub owner_id: String,
    /// Human-readable name for the key.
    pub name: String,
    /// SHA-256 hash of the plaintext key.
    pub key_hash: String,
    /// Short prefix shown for identification (e.g. `sk_live_a1b2`).
    pub key_prefix: String,
    /// Scopes the key is authorised for.
    pub scopes: Vec<String>,
    /// Whether the key is currently active.
    pub is_active: bool,
    /// When the key expires (`None` = never).
    pub expires_at: Option<DateTime<Utc>>,
    /// End of the grace period after rotation (`None` = no grace).
    pub grace_ends_at: Option<DateTime<Utc>>,
    /// ID of the replacement key if this key was rotated.
    pub rotated_by_id: Option<String>,
    /// Timestamp of the last successful validation.
    pub last_used_at: Option<DateTime<Utc>>,
    /// When this key was created.
    pub created_at: DateTime<Utc>,
}

impl Key {
    /// Return true if the key is expired and beyond its grace period.
    pub fn is_expired_past_grace(&self) -> bool {
        let now = Utc::now();
        if let Some(grace_ends_at) = self.grace_ends_at {
            if now > grace_ends_at {
                return true;
            }
        }
        if let Some(expires_at) = self.expires_at {
            if now > expires_at && self.grace_ends_at.is_none() {
                return true;
            }
        }
        false
    }
}

/// Result of key generation — contains the plaintext shown once.
#[derive(Debug, Clone, zeroize::Zeroize, zeroize::ZeroizeOnDrop)]
pub struct GenerateResult {
    /// The full plaintext key (show once, then discard).
    pub plain_key: String,
    /// SHA-256 hash to store in the database.
    pub key_hash: String,
    /// Short display prefix for user-facing logs.
    pub prefix: String,
}

/// Generate a new random API key with the given prefix.
///
/// The key is `prefix` + 32 hex characters (16 random bytes).
/// Example: `generate("sk_live_")` produces `"sk_live_a1b2c3d4e5f6..."`
pub fn generate(prefix: &str) -> GenerateResult {
    let mut random_bytes = [0u8; 16];
    rand::fill(&mut random_bytes);
    let random_hex = hex::encode(random_bytes);
    let plain_key = format!("{}{}", prefix, random_hex);
    let key_hash = hash_key(&plain_key);
    let display_prefix = if plain_key.len() > 8 {
        plain_key[..8].to_string()
    } else {
        plain_key.clone()
    };

    GenerateResult {
        plain_key,
        key_hash,
        prefix: display_prefix,
    }
}

/// Return the SHA-256 hex digest of a plaintext API key.
pub fn hash_key(plain_key: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(plain_key.as_bytes());
    hex::encode(hasher.finalize())
}

/// Perform a constant-time comparison between the hash of a plaintext key
/// and a stored hash. Returns `true` if they match.
///
/// This prevents timing attacks that could leak information about stored hashes.
pub fn compare_hash(plain_key: &str, stored_hash: &str) -> bool {
    use subtle::ConstantTimeEq;
    let computed = hash_key(plain_key);
    computed.as_bytes().ct_eq(stored_hash.as_bytes()).into()
}

/// Error returned when an API key fails validation.
#[derive(Debug, Clone, thiserror::Error)]
pub enum KeyValidationError {
    /// The key has been explicitly revoked.
    #[error("key is revoked")]
    Revoked,
    /// The key has expired (and any grace period has ended).
    #[error("key is expired")]
    Expired,
}

/// Check that a key is usable (active and not expired past grace).
pub fn validate(key: &Key) -> Result<(), KeyValidationError> {
    if !key.is_active {
        return Err(KeyValidationError::Revoked);
    }
    if key.is_expired_past_grace() {
        return Err(KeyValidationError::Expired);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compare_hash_matching() {
        let key = "sk_test_abc123def456";
        let hash = hash_key(key);
        assert!(compare_hash(key, &hash));
    }

    #[test]
    fn compare_hash_wrong_key() {
        let hash = hash_key("sk_test_abc123def456");
        assert!(!compare_hash("sk_test_wrong", &hash));
    }

    #[test]
    fn compare_hash_wrong_hash() {
        assert!(!compare_hash("sk_test_abc123def456", "badhash"));
    }
}
