use argon2::{
    Algorithm, Argon2, Params, Version,
    password_hash::{self, PasswordHasher as Argon2Hasher, PasswordVerifier, phc::PasswordHash},
};
use rskit_errors::{AppError, AppResult};

const ARGON2_MEMORY_COST_KIB: u32 = 65_536;
const ARGON2_TIME_COST: u32 = 3;
const ARGON2_PARALLELISM: u32 = 4;
const ARGON2_HASH_LEN: usize = 32;

/// Supported password hashing algorithms.
#[derive(Debug, Clone, Default)]
#[non_exhaustive]
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
    pub const fn new(algorithm: HashAlgorithm) -> Self {
        Self { algorithm }
    }

    /// Hash a plaintext `password`.
    pub fn hash(&self, password: &str) -> AppResult<String> {
        configured_argon2()?
            .hash_password(password.as_bytes())
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
        match configured_argon2()?.verify_password(password.as_bytes(), &parsed) {
            Ok(()) => Ok(true),
            Err(password_hash::Error::PasswordInvalid) => Ok(false),
            Err(e) => Err(AppError::new(
                rskit_errors::ErrorCode::Internal,
                format!("password verification error: {e}"),
            )),
        }
    }
}

fn configured_argon2() -> AppResult<Argon2<'static>> {
    let params = Params::new(
        ARGON2_MEMORY_COST_KIB,
        ARGON2_TIME_COST,
        ARGON2_PARALLELISM,
        Some(ARGON2_HASH_LEN),
    )
    .map_err(|_e| AppError::internal(std::io::Error::other("invalid argon2 params")))?;
    Ok(Argon2::new(Algorithm::Argon2id, Version::V0x13, params))
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

    #[test]
    fn hash_uses_locked_group_05_parameters() {
        let hash = PasswordHasher::default().hash("hunter2hunter2").unwrap();
        assert!(hash.contains("$argon2id$"));
        assert!(hash.contains("m=65536"));
        assert!(hash.contains("t=3"));
        assert!(hash.contains("p=4"));
    }

    #[test]
    fn verify_reports_unsupported_hash_as_error_not_mismatch() {
        let pbkdf2 =
            "$pbkdf2-sha256$i=600000$c2FsdHNhbHRzYWx0$Pq3SFyw1h7f6v0n0kGfn0p0r2O6Xn1sQq4mT8Ckz9xA";
        let err = PasswordHasher::default()
            .verify("hunter2", pbkdf2)
            .unwrap_err();
        assert_eq!(err.code(), rskit_errors::ErrorCode::Internal);
    }
}
