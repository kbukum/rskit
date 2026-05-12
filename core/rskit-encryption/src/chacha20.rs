//! ChaCha20-Poly1305 encryption implementation.

use base64::{Engine, engine::general_purpose::STANDARD};
use chacha20poly1305::{
    ChaCha20Poly1305, Nonce,
    aead::{Aead, KeyInit, Payload},
};
use rand::Rng;
use rskit_errors::{AppError, AppResult, ErrorCode};
use sha2::Sha256;
use zeroize::Zeroize;

use crate::traits::{Algorithm, Encryptor};

const SALT_SIZE: usize = 16;
const NONCE_SIZE: usize = 12;
const PBKDF2_ITERATIONS: u32 = 600_000;
const KEY_LEN: usize = 32;

/// ChaCha20-Poly1305 encryptor.
///
/// Uses PBKDF2-SHA256 for key derivation with a random 16-byte salt per encryption.
/// Output format: `base64(salt[16] || nonce[12] || ciphertext)`.
pub struct ChaCha20Encryptor {
    passphrase: Vec<u8>,
}

impl ChaCha20Encryptor {
    /// Creates a new ChaCha20-Poly1305 encryptor.
    ///
    /// The passphrase is stored and used with PBKDF2-SHA256 to derive keys per operation.
    pub fn new(key: &[u8]) -> Self {
        Self {
            passphrase: key.to_vec(),
        }
    }

    fn derive_key(&self, salt: &[u8]) -> [u8; KEY_LEN] {
        let mut key = [0u8; KEY_LEN];
        pbkdf2::pbkdf2_hmac::<Sha256>(&self.passphrase, salt, PBKDF2_ITERATIONS, &mut key);
        key
    }
}

impl Drop for ChaCha20Encryptor {
    fn drop(&mut self) {
        self.passphrase.zeroize();
    }
}

impl Encryptor for ChaCha20Encryptor {
    fn encrypt(&self, plaintext: &[u8]) -> AppResult<String> {
        let mut salt = [0u8; SALT_SIZE];
        rand::rng().fill_bytes(&mut salt);

        let mut key_bytes = self.derive_key(&salt);
        let cipher = ChaCha20Poly1305::new((&key_bytes[..]).into());
        key_bytes.zeroize();

        let mut nonce_bytes = [0u8; NONCE_SIZE];
        rand::rng().fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);

        let ciphertext = cipher
            .encrypt(nonce, Payload::from(plaintext))
            .map_err(|e| {
                AppError::new(
                    ErrorCode::Internal,
                    format!("ChaCha20-Poly1305 encryption failed: {e}"),
                )
            })?;

        let mut result = Vec::with_capacity(SALT_SIZE + NONCE_SIZE + ciphertext.len());
        result.extend_from_slice(&salt);
        result.extend_from_slice(&nonce_bytes);
        result.extend_from_slice(&ciphertext);

        Ok(STANDARD.encode(&result))
    }

    fn decrypt(&self, ciphertext: &str) -> AppResult<Vec<u8>> {
        let data = STANDARD.decode(ciphertext).map_err(|e| {
            AppError::new(
                ErrorCode::InvalidFormat,
                format!("Invalid base64 ciphertext: {e}"),
            )
        })?;

        if data.len() < SALT_SIZE + NONCE_SIZE {
            return Err(AppError::new(
                ErrorCode::InvalidFormat,
                "Ciphertext too short (missing salt or nonce)",
            ));
        }

        let (salt, remaining) = data.split_at(SALT_SIZE);
        let (nonce_bytes, cipher_bytes) = remaining.split_at(NONCE_SIZE);

        let mut key_bytes = self.derive_key(salt);
        let cipher = ChaCha20Poly1305::new((&key_bytes[..]).into());
        key_bytes.zeroize();

        let nonce = Nonce::from_slice(nonce_bytes);

        cipher
            .decrypt(nonce, Payload::from(cipher_bytes))
            .map_err(|e| {
                AppError::new(
                    ErrorCode::InvalidFormat,
                    format!("ChaCha20-Poly1305 decryption failed: {e}"),
                )
            })
    }

    fn algorithm(&self) -> Algorithm {
        Algorithm::ChaCha20Poly1305
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let encryptor = ChaCha20Encryptor::new(b"my-secret-key");
        let plaintext = b"Hello, World!";

        let ciphertext = encryptor.encrypt(plaintext).unwrap();
        let decrypted = encryptor.decrypt(&ciphertext).unwrap();

        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_encrypt_produces_different_ciphertext() {
        let encryptor = ChaCha20Encryptor::new(b"my-secret-key");
        let plaintext = b"Same plaintext";

        let ct1 = encryptor.encrypt(plaintext).unwrap();
        let ct2 = encryptor.encrypt(plaintext).unwrap();

        assert_ne!(ct1, ct2);
    }

    #[test]
    fn test_decrypt_with_wrong_key_fails() {
        let encryptor1 = ChaCha20Encryptor::new(b"key-1");
        let encryptor2 = ChaCha20Encryptor::new(b"key-2");

        let plaintext = b"Secret data";
        let ciphertext = encryptor1.encrypt(plaintext).unwrap();

        let result = encryptor2.decrypt(&ciphertext);
        assert!(result.is_err());
    }

    #[test]
    fn test_encrypt_empty_plaintext() {
        let encryptor = ChaCha20Encryptor::new(b"my-secret-key");
        let plaintext = b"";

        let ciphertext = encryptor.encrypt(plaintext).unwrap();
        let decrypted = encryptor.decrypt(&ciphertext).unwrap();

        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_algorithm() {
        let encryptor = ChaCha20Encryptor::new(b"my-secret-key");
        assert_eq!(encryptor.algorithm(), Algorithm::ChaCha20Poly1305);
    }

    #[test]
    fn test_invalid_ciphertext() {
        let encryptor = ChaCha20Encryptor::new(b"my-secret-key");
        let result = encryptor.decrypt("invalid-base64!@#$");
        assert!(result.is_err());
    }

    #[test]
    fn test_corrupted_ciphertext() {
        let encryptor = ChaCha20Encryptor::new(b"my-secret-key");
        let plaintext = b"Original";
        let ciphertext = encryptor.encrypt(plaintext).unwrap();

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
