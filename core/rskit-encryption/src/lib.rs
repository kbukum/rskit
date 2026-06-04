#![warn(missing_docs)]
//! Symmetric encryption utilities with AES-256-GCM and ChaCha20-Poly1305 support.
//!
//! This crate provides a unified interface for symmetric encryption and decryption
//! of sensitive data using either AES-256-GCM (default, with hardware acceleration)
//! or ChaCha20-Poly1305 (modern, performant without AES-NI).
//!
//! Keys are derived from passphrases using PBKDF2-SHA256 with 600,000 iterations
//! and a random 16-byte salt per encryption operation.
//!
//! Ciphertext format:
//! `base64(version[1] || algorithm[1] || salt[16] || nonce[12] || ciphertext)`.
//!
//! # Examples
//!
//! ```no_run
//! use rskit_encryption::{Encryptor, Algorithm};
//! use rskit_encryption::new_encryptor;
//!
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let encryptor = new_encryptor(b"my-secret-key", Algorithm::AesGcm);
//!
//! let plaintext = b"sensitive data";
//! let ciphertext = encryptor.encrypt(plaintext)?;
//! let decrypted = encryptor.decrypt(&ciphertext)?;
//!
//! assert_eq!(decrypted, plaintext);
//! # Ok(())
//! # }
//! ```

pub mod aes_gcm;
pub mod chacha20;
pub mod traits;

mod envelope;

pub use aes_gcm::AesGcmEncryptor;
pub use chacha20::ChaCha20Encryptor;
pub use traits::{Algorithm, Encryptor};

/// Creates a new encryptor for the specified algorithm.
///
/// # Arguments
///
/// * `key` - The encryption passphrase (used with PBKDF2-SHA256 to derive keys)
/// * `algorithm` - The encryption algorithm to use
///
/// # Returns
///
/// A boxed trait object implementing [`Encryptor`].
///
/// # Examples
///
/// ```no_run
/// use rskit_encryption::{new_encryptor, Algorithm};
///
/// let encryptor = new_encryptor(b"secret-key", Algorithm::ChaCha20Poly1305);
/// ```
pub fn new_encryptor(key: &[u8], algorithm: Algorithm) -> Box<dyn Encryptor> {
    match algorithm {
        Algorithm::AesGcm => Box::new(AesGcmEncryptor::new(key)),
        Algorithm::ChaCha20Poly1305 => Box::new(ChaCha20Encryptor::new(key)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_factory_aes_gcm() {
        let encryptor = new_encryptor(b"secret-key", Algorithm::AesGcm);
        assert_eq!(encryptor.algorithm(), Algorithm::AesGcm);

        let plaintext = b"test";
        let ciphertext = encryptor.encrypt(plaintext).unwrap();
        let decrypted = encryptor.decrypt(&ciphertext).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_factory_chacha20() {
        let encryptor = new_encryptor(b"secret-key", Algorithm::ChaCha20Poly1305);
        assert_eq!(encryptor.algorithm(), Algorithm::ChaCha20Poly1305);

        let plaintext = b"test";
        let ciphertext = encryptor.encrypt(plaintext).unwrap();
        let decrypted = encryptor.decrypt(&ciphertext).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn malformed_versioned_envelopes_are_rejected() {
        use base64::{Engine, engine::general_purpose::STANDARD};

        let encryptor = new_encryptor(b"secret-key", Algorithm::AesGcm);
        let too_short = STANDARD.encode([1_u8, 1, 2, 3]);
        let err = encryptor.decrypt(&too_short).unwrap_err();
        assert_eq!(err.code(), rskit_errors::ErrorCode::InvalidFormat);

        let mut bad_version = vec![0_u8, 1];
        bad_version.extend_from_slice(&[0_u8; 44]);
        let err = encryptor
            .decrypt(&STANDARD.encode(bad_version))
            .unwrap_err();
        assert_eq!(err.code(), rskit_errors::ErrorCode::InvalidFormat);

        let mut bad_algorithm = vec![1_u8, 99];
        bad_algorithm.extend_from_slice(&[0_u8; 44]);
        let err = encryptor
            .decrypt(&STANDARD.encode(bad_algorithm))
            .unwrap_err();
        assert_eq!(err.code(), rskit_errors::ErrorCode::InvalidFormat);
    }
}
