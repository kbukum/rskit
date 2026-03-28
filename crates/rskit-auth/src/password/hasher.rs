use argon2::{
    Argon2,
    password_hash::{PasswordHash, PasswordHasher as Argon2Hasher, PasswordVerifier, SaltString},
};
use rskit_errors::{AppError, AppResult};

/// Supported password hashing algorithms.
#[derive(Debug, Clone, Default)]
pub enum HashAlgorithm {
    /// Argon2id — recommended default.
    #[default]
    Argon2id,
}

/// Hashes and verifies passwords.
#[derive(Debug, Clone, Default)]
pub struct PasswordHasher {
    /// Hashing algorithm to use.
    pub algorithm: HashAlgorithm,
}

impl PasswordHasher {
    /// Create a new [`PasswordHasher`] with the given algorithm.
    pub fn new(algorithm: HashAlgorithm) -> Self {
        Self { algorithm }
    }

    /// Hash a plaintext `password`.
    pub fn hash(&self, password: &str) -> AppResult<String> {
        let mut rng = argon2::password_hash::rand_core::OsRng;
        let salt = SaltString::generate(&mut rng);
        let argon2 = Argon2::default();
        argon2
            .hash_password(password.as_bytes(), &salt)
            .map(|h| h.to_string())
            .map_err(|e| {
                AppError::new(
                    rskit_errors::ErrorCode::Internal,
                    format!("password hash error: {e}"),
                )
            })
    }

    /// Verify that `password` matches the stored `hash`.
    pub fn verify(&self, password: &str, hash: &str) -> AppResult<bool> {
        let parsed = PasswordHash::new(hash).map_err(|e| {
            AppError::new(
                rskit_errors::ErrorCode::Internal,
                format!("invalid hash format: {e}"),
            )
        })?;
        Ok(Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_and_verify_roundtrip() {
        let h = PasswordHasher::default();
        let hash = h.hash("hunter2").unwrap();
        assert!(h.verify("hunter2", &hash).unwrap());
        assert!(!h.verify("wrong", &hash).unwrap());
    }
}
