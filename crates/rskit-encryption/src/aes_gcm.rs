//! AES-256-GCM encryption implementation.

use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{Aead, KeyInit, Payload},
};
use base64::{Engine, engine::general_purpose::STANDARD};
use rand::Rng;
use rskit_errors::{AppError, AppResult, ErrorCode};
use sha2::{Digest, Sha256};

use crate::traits::{Algorithm, Encryptor};

/// AES-256-GCM encryptor.
///
/// Uses a random 12-byte nonce for each encryption, prepended to the ciphertext.
/// The output is base64-encoded for safe transmission.
pub struct AesGcmEncryptor {
    cipher: Aes256Gcm,
}

impl AesGcmEncryptor {
    /// Creates a new AES-256-GCM encryptor.
    ///
    /// The key is hashed with SHA-256 to ensure it is exactly 32 bytes.
    ///
    /// # Errors
    ///
    /// Returns an error if the cipher cannot be created (should not occur with valid key).
    pub fn new(key: &[u8]) -> AppResult<Self> {
        let mut hasher = Sha256::new();
        hasher.update(key);
        let key_bytes = hasher.finalize();

        let cipher = Aes256Gcm::new((&key_bytes[..]).into());

        Ok(Self { cipher })
    }
}

impl Encryptor for AesGcmEncryptor {
    fn encrypt(&self, plaintext: &[u8]) -> AppResult<String> {
        let mut nonce_bytes = [0u8; 12];
        rand::rng().fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        let ciphertext = self
            .cipher
            .encrypt(nonce, Payload::from(plaintext))
            .map_err(|e| {
                AppError::new(
                    ErrorCode::Internal,
                    format!("AES-GCM encryption failed: {}", e),
                )
            })?;

        // Prepend nonce to ciphertext
        let mut result = nonce_bytes.to_vec();
        result.extend_from_slice(&ciphertext);

        Ok(STANDARD.encode(&result))
    }

    fn decrypt(&self, ciphertext: &str) -> AppResult<Vec<u8>> {
        let data = STANDARD.decode(ciphertext).map_err(|e| {
            AppError::new(
                ErrorCode::InvalidFormat,
                format!("Invalid base64 ciphertext: {}", e),
            )
        })?;

        const NONCE_SIZE: usize = 12;
        if data.len() < NONCE_SIZE {
            return Err(AppError::new(
                ErrorCode::InvalidFormat,
                "Ciphertext too short (missing nonce)",
            ));
        }

        let (nonce_bytes, cipher_bytes) = data.split_at(NONCE_SIZE);
        let nonce = Nonce::from_slice(nonce_bytes);

        self.cipher
            .decrypt(nonce, Payload::from(cipher_bytes))
            .map_err(|e| {
                AppError::new(
                    ErrorCode::InvalidFormat,
                    format!("AES-GCM decryption failed: {}", e),
                )
            })
    }

    fn algorithm(&self) -> Algorithm {
        Algorithm::AesGcm
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let encryptor = AesGcmEncryptor::new(b"my-secret-key").unwrap();
        let plaintext = b"Hello, World!";

        let ciphertext = encryptor.encrypt(plaintext).unwrap();
        let decrypted = encryptor.decrypt(&ciphertext).unwrap();

        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_encrypt_produces_different_ciphertext() {
        let encryptor = AesGcmEncryptor::new(b"my-secret-key").unwrap();
        let plaintext = b"Same plaintext";

        let ct1 = encryptor.encrypt(plaintext).unwrap();
        let ct2 = encryptor.encrypt(plaintext).unwrap();

        // Different nonces should produce different ciphertexts
        assert_ne!(ct1, ct2);
    }

    #[test]
    fn test_decrypt_with_wrong_key_fails() {
        let encryptor1 = AesGcmEncryptor::new(b"key-1").unwrap();
        let encryptor2 = AesGcmEncryptor::new(b"key-2").unwrap();

        let plaintext = b"Secret data";
        let ciphertext = encryptor1.encrypt(plaintext).unwrap();

        let result = encryptor2.decrypt(&ciphertext);
        assert!(result.is_err());
    }

    #[test]
    fn test_encrypt_empty_plaintext() {
        let encryptor = AesGcmEncryptor::new(b"my-secret-key").unwrap();
        let plaintext = b"";

        let ciphertext = encryptor.encrypt(plaintext).unwrap();
        let decrypted = encryptor.decrypt(&ciphertext).unwrap();

        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_algorithm() {
        let encryptor = AesGcmEncryptor::new(b"my-secret-key").unwrap();
        assert_eq!(encryptor.algorithm(), Algorithm::AesGcm);
    }

    #[test]
    fn test_invalid_ciphertext() {
        let encryptor = AesGcmEncryptor::new(b"my-secret-key").unwrap();
        let result = encryptor.decrypt("invalid-base64!@#$");
        assert!(result.is_err());
    }

    #[test]
    fn test_corrupted_ciphertext() {
        let encryptor = AesGcmEncryptor::new(b"my-secret-key").unwrap();
        let plaintext = b"Original";
        let ciphertext = encryptor.encrypt(plaintext).unwrap();

        // Corrupt the ciphertext by modifying a byte in the middle
        let mut corrupted = ciphertext.clone();
        let chars: Vec<char> = corrupted.chars().collect();
        if chars.len() > 20 {
            let mut new_chars = chars.clone();
            new_chars[20] = if chars[20] == 'A' { 'B' } else { 'A' };
            corrupted = new_chars.into_iter().collect();
        }

        let result = encryptor.decrypt(&corrupted);
        assert!(result.is_err());
    }
}
