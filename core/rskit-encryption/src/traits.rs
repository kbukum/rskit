//! Core encryption traits and types.

use rskit_errors::AppResult;

/// Supported encryption algorithms.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Algorithm {
    /// AES-256-GCM (default, widely supported with hardware acceleration)
    AesGcm,
    /// ChaCha20-Poly1305 (modern, performant on CPUs without AES-NI)
    ChaCha20Poly1305,
}

impl Algorithm {
    /// Returns the algorithm name as a string.
    pub fn name(&self) -> &'static str {
        match self {
            Self::AesGcm => "aes-256-gcm",
            Self::ChaCha20Poly1305 => "chacha20-poly1305",
        }
    }
}

impl std::fmt::Display for Algorithm {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}

/// Trait for symmetric encryption and decryption operations.
///
/// Implementations are thread-safe and can be shared across async boundaries.
pub trait Encryptor: Send + Sync {
    /// Encrypts plaintext and returns base64-encoded ciphertext.
    ///
    /// The ciphertext includes a random nonce prepended to the encrypted data,
    /// so each call to encrypt with the same plaintext produces different output.
    fn encrypt(&self, plaintext: &[u8]) -> AppResult<String>;

    /// Decrypts base64-encoded ciphertext and returns the plaintext.
    fn decrypt(&self, ciphertext: &str) -> AppResult<Vec<u8>>;

    /// Returns the algorithm used by this encryptor.
    fn algorithm(&self) -> Algorithm;
}
