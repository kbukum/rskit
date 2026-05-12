//! Argon2id password hashing implementation.

use argon2::{
    Argon2, PasswordHash, PasswordHasher, PasswordVerifier,
    password_hash::{SaltString, rand_core::OsRng},
};
use rskit_errors::{AppError, AppResult, ErrorCode};

use crate::traits::{Algorithm, Encryptor};

/// Argon2id hasher.
///
/// Implements [`Encryptor`] but only for one-way hashing (decrypt returns error).
pub struct Argon2idHasher {
    // Argon2id is typically used for hashing, not symmetric encryption in this context.
    // However, we satisfy the Encryptor trait by providing hash as "encrypt".
}

impl Argon2idHasher {
    /// Creates a new Argon2id hasher.
    pub fn new() -> Self {
        Self {}
    }
}

impl Default for Argon2idHasher {
    fn default() -> Self {
        Self::new()
    }
}

impl Encryptor for Argon2idHasher {
    fn encrypt(&self, plaintext: &[u8]) -> AppResult<String> {
        let salt = SaltString::generate(&mut OsRng);
        let argon2 = Argon2::default();

        let password_hash = argon2
            .hash_password(plaintext, &salt)
            .map_err(|e| {
                AppError::new(
                    ErrorCode::Internal,
                    format!("Argon2id hashing failed: {e}"),
                )
            })?
            .to_string();

        Ok(password_hash)
    }

    fn decrypt(&self, _ciphertext: &str) -> AppResult<Vec<u8>> {
        Err(AppError::new(
            ErrorCode::Internal,
            "Argon2id is a one-way hash and does not support decryption",
        ))
    }

    fn algorithm(&self) -> Algorithm {
        Algorithm::Argon2id
    }
}

/// Verify a plaintext against an Argon2id hash.
pub fn verify_hash(hash: &str, plaintext: &[u8]) -> AppResult<bool> {
    let parsed_hash = PasswordHash::new(hash).map_err(|e| {
        AppError::new(
            ErrorCode::InvalidFormat,
            format!("Invalid Argon2id hash format: {e}"),
        )
    })?;

    Ok(Argon2::default()
        .verify_password(plaintext, &parsed_hash)
        .is_ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_argon2id_hashing() {
        let hasher = Argon2idHasher::new();
        let password = b"hunter2";

        let hash = hasher.encrypt(password).unwrap();
        assert!(hash.contains("$argon2id$"));

        assert!(verify_hash(&hash, password).unwrap());
        assert!(!verify_hash(&hash, b"wrong-password").unwrap());
    }

    #[test]
    fn test_decrypt_fails() {
        let hasher = Argon2idHasher::new();
        let result = hasher.decrypt("some-hash");
        assert!(result.is_err());
    }
}
