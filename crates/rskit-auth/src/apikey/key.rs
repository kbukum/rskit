//! API key data structure, generation, hashing, and validation.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

/// API key metadata (never stores the plaintext).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Key {
    pub id: String,
    pub owner_id: String,
    pub name: String,
    pub key_hash: String,
    pub key_prefix: String,
    pub scopes: Vec<String>,
    pub is_active: bool,
    pub expires_at: Option<DateTime<Utc>>,
    pub grace_ends_at: Option<DateTime<Utc>>,
    pub rotated_by_id: Option<String>,
    pub last_used_at: Option<DateTime<Utc>>,
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
#[derive(Debug, Clone)]
pub struct GenerateResult {
    pub plain_key: String,
    pub key_hash: String,
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

/// Error returned when an API key fails validation.
#[derive(Debug, Clone, thiserror::Error)]
pub enum KeyValidationError {
    #[error("key is revoked")]
    Revoked,
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
