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

mod factory;

pub use factory::new_encryptor;

#[cfg(test)]
mod tests;
