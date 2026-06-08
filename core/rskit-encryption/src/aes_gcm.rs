//! AES-256-GCM encryption implementation.

use aes_gcm::{
    Aes256Gcm, Nonce,
    aead::{Aead, KeyInit, Payload},
};
use rand::RngCore;
use rskit_errors::{AppError, AppResult, ErrorCode};
use sha2::Sha256;
use zeroize::Zeroize;

use crate::envelope::{Envelope, KEY_LEN, NONCE_SIZE, PBKDF2_ITERATIONS, SALT_SIZE};
use crate::traits::{Algorithm, Encryptor};

/// AES-256-GCM encryptor.
///
/// Uses PBKDF2-SHA256 for key derivation with a random 16-byte salt per encryption.
/// Output format: `base64(version[1] || algorithm[1] || salt[16] || nonce[12] || ciphertext)`.
pub struct AesGcmEncryptor {
    passphrase: Vec<u8>,
}

impl AesGcmEncryptor {
    /// Creates a new AES-256-GCM encryptor.
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

impl Drop for AesGcmEncryptor {
    fn drop(&mut self) {
        self.passphrase.zeroize();
    }
}

impl Encryptor for AesGcmEncryptor {
    fn encrypt(&self, plaintext: &[u8]) -> AppResult<String> {
        let mut salt = [0u8; SALT_SIZE];
        rand::rng().fill_bytes(&mut salt);

        let mut key_bytes = self.derive_key(&salt);
        let cipher = Aes256Gcm::new((&key_bytes[..]).into());
        key_bytes.zeroize();

        let mut nonce_bytes = [0u8; NONCE_SIZE];
        rand::rng().fill_bytes(&mut nonce_bytes);
        let nonce = Nonce::from_slice(&nonce_bytes);
        let aad = crate::envelope::associated_data(Algorithm::AesGcm, &salt, &nonce_bytes);

        let ciphertext = cipher
            .encrypt(
                nonce,
                Payload {
                    msg: plaintext,
                    aad: &aad,
                },
            )
            .map_err(|_| AppError::new(ErrorCode::Internal, "encryption failed"))?;

        Ok(Envelope::encode(
            Algorithm::AesGcm,
            &salt,
            &nonce_bytes,
            &ciphertext,
        ))
    }

    fn decrypt(&self, ciphertext: &str) -> AppResult<Vec<u8>> {
        let envelope = Envelope::decode(ciphertext)?;
        if envelope.algorithm()? != Algorithm::AesGcm {
            return Err(AppError::new(
                ErrorCode::InvalidFormat,
                "ciphertext algorithm does not match encryptor",
            ));
        }

        let mut key_bytes = self.derive_key(envelope.salt());
        let cipher = Aes256Gcm::new((&key_bytes[..]).into());
        key_bytes.zeroize();

        let nonce = Nonce::from_slice(envelope.nonce());

        cipher
            .decrypt(
                nonce,
                Payload {
                    msg: envelope.ciphertext(),
                    aad: envelope.associated_data(),
                },
            )
            .map_err(|_| {
                AppError::new(ErrorCode::InvalidFormat, "ciphertext authentication failed")
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
