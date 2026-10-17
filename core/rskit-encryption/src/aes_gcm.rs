//! AES-256-GCM encryption implementation.

use rskit_errors::AppResult;

use crate::key::MasterKey;
use crate::traits::{Algorithm, Encryptor};

/// AES-256-GCM encryptor.
///
/// Each encryption derives a fresh message key from a random 16-byte salt. Output format:
/// `base64(version[1] || algorithm[1] || salt[16] || nonce[12] || ciphertext)`, where version 1
/// means a PBKDF2-SHA256 passphrase and version 2 means HKDF-SHA256 key material.
pub struct AesGcmEncryptor {
    key: MasterKey,
}

impl AesGcmEncryptor {
    /// Creates a AES-256-GCM encryptor from a passphrase, stretched with PBKDF2-SHA256 per operation.
    pub fn new(key: &[u8]) -> Self {
        Self {
            key: MasterKey::passphrase(key),
        }
    }

    /// Creates a AES-256-GCM encryptor from uniformly random key material of at least 32 bytes,
    /// such as a mounted key file. Message keys come from HKDF-SHA256, so operations are cheap.
    ///
    /// # Errors
    ///
    /// Returns `InvalidInput` when the key is shorter than 32 bytes.
    pub fn from_key_material(key: &[u8]) -> AppResult<Self> {
        Ok(Self {
            key: MasterKey::key_material(key)?,
        })
    }
}

impl Encryptor for AesGcmEncryptor {
    fn encrypt(&self, plaintext: &[u8]) -> AppResult<String> {
        self.key.seal(
            Algorithm::AesGcm,
            plaintext,
            &rand::random(),
            &rand::random(),
        )
    }

    fn decrypt(&self, ciphertext: &str) -> AppResult<Vec<u8>> {
        self.key.open(Algorithm::AesGcm, ciphertext)
    }

    fn algorithm(&self) -> Algorithm {
        Algorithm::AesGcm
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rskit_errors::ErrorCode;

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let encryptor = AesGcmEncryptor::new(b"my-secret-key");
        let plaintext = b"Hello, World!";

        let ciphertext = encryptor.encrypt(plaintext).unwrap();
        let decrypted = encryptor.decrypt(&ciphertext).unwrap();

        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_encrypt_produces_different_ciphertext() {
        let encryptor = AesGcmEncryptor::new(b"my-secret-key");
        let plaintext = b"Same plaintext";

        let ct1 = encryptor.encrypt(plaintext).unwrap();
        let ct2 = encryptor.encrypt(plaintext).unwrap();

        assert_ne!(ct1, ct2);
    }

    #[test]
    fn test_decrypt_with_wrong_key_fails() {
        let encryptor1 = AesGcmEncryptor::new(b"key-1");
        let encryptor2 = AesGcmEncryptor::new(b"key-2");

        let plaintext = b"Secret data";
        let ciphertext = encryptor1.encrypt(plaintext).unwrap();

        let result = encryptor2.decrypt(&ciphertext);
        assert!(result.is_err());
    }

    #[test]
    fn test_encrypt_empty_plaintext() {
        let encryptor = AesGcmEncryptor::new(b"my-secret-key");
        let plaintext = b"";

        let ciphertext = encryptor.encrypt(plaintext).unwrap();
        let decrypted = encryptor.decrypt(&ciphertext).unwrap();

        assert_eq!(decrypted, plaintext);
    }

    #[test]
    fn test_algorithm() {
        let encryptor = AesGcmEncryptor::new(b"my-secret-key");
        assert_eq!(encryptor.algorithm(), Algorithm::AesGcm);
    }

    #[test]
    fn test_invalid_ciphertext() {
        let encryptor = AesGcmEncryptor::new(b"my-secret-key");
        let result = encryptor.decrypt("invalid-base64!@#$");
        assert!(result.is_err());
    }

    #[test]
    fn test_rejects_ciphertext_for_other_algorithm() {
        let encryptor = AesGcmEncryptor::new(b"my-secret-key");
        let other = crate::ChaCha20Encryptor::new(b"my-secret-key");
        let ciphertext = other.encrypt(b"secret").unwrap();

        let err = encryptor.decrypt(&ciphertext).unwrap_err();
        assert_eq!(err.code(), ErrorCode::InvalidFormat);
    }

    #[test]
    fn test_corrupted_ciphertext() {
        let encryptor = AesGcmEncryptor::new(b"my-secret-key");
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
