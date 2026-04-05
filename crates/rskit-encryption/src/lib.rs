#![warn(missing_docs)]
//! Symmetric encryption utilities with AES-256-GCM and ChaCha20-Poly1305 support.
//!
//! This crate provides a unified interface for symmetric encryption and decryption
//! of sensitive data using either AES-256-GCM (default, with hardware acceleration)
//! or ChaCha20-Poly1305 (modern, performant without AES-NI).
//!
//! # Examples
//!
//! ```no_run
//! use rskit_encryption::{Encryptor, Algorithm};
//! use rskit_encryption::new_encryptor;
//!
//! # #[tokio::main]
//! # async fn main() -> Result<(), Box<dyn std::error::Error>> {
//! // Create an AES-GCM encryptor (default)
//! let encryptor = new_encryptor(b"my-secret-key", Algorithm::AesGcm)?;
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

pub use aes_gcm::AesGcmEncryptor;
pub use chacha20::ChaCha20Encryptor;
pub use traits::{Algorithm, Encryptor};

use rskit_errors::AppResult;

/// Creates a new encryptor for the specified algorithm.
///
/// # Arguments
///
/// * `key` - The encryption key (will be hashed to 32 bytes with SHA-256)
/// * `algorithm` - The encryption algorithm to use
///
/// # Returns
///
/// A boxed trait object implementing [`Encryptor`].
///
/// # Errors
///
/// Returns an error if the encryptor cannot be created.
///
/// # Examples
///
/// ```no_run
/// use rskit_encryption::{new_encryptor, Algorithm};
///
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let encryptor = new_encryptor(b"secret-key", Algorithm::ChaCha20Poly1305)?;
/// # Ok(())
/// # }
/// ```
pub fn new_encryptor(key: &[u8], algorithm: Algorithm) -> AppResult<Box<dyn Encryptor>> {
    match algorithm {
        Algorithm::AesGcm => Ok(Box::new(AesGcmEncryptor::new(key)?)),
        Algorithm::ChaCha20Poly1305 => Ok(Box::new(ChaCha20Encryptor::new(key)?)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_factory_aes_gcm() {
        let encryptor = new_encryptor(b"secret-key", Algorithm::AesGcm).unwrap();
        assert_eq!(encryptor.algorithm(), Algorithm::AesGcm);

        let plaintext = b"test";
        let ciphertext = encryptor.encrypt(plaintext).unwrap();
        let decrypted = encryptor.decrypt(&ciphertext).unwrap();
        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_factory_chacha20() {
        let encryptor = new_encryptor(b"secret-key", Algorithm::ChaCha20Poly1305).unwrap();
        assert_eq!(encryptor.algorithm(), Algorithm::ChaCha20Poly1305);

        let plaintext = b"test";
        let ciphertext = encryptor.encrypt(plaintext).unwrap();
        let decrypted = encryptor.decrypt(&ciphertext).unwrap();
        assert_eq!(decrypted, plaintext);
    }
}
