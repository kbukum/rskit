//! Master key handling shared by both encryptors.

use aes_gcm::{
    Aes256Gcm,
    aead::{Aead, KeyInit, Payload},
};
use chacha20poly1305::ChaCha20Poly1305;
use hkdf::Hkdf;
use rskit_errors::{AppError, AppResult, ErrorCode};
use sha2::Sha256;
use zeroize::Zeroize;

use crate::Algorithm;
use crate::envelope::{Envelope, KEY_LEN, NONCE_SIZE, PBKDF2_ITERATIONS, SALT_SIZE};

/// Shortest key accepted as key material.
pub(crate) const MIN_KEY_MATERIAL: usize = 32;

/// How a message key is derived from the master key; recorded as the envelope version.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Derivation {
    /// A human-chosen passphrase stretched with PBKDF2-SHA256 (version 1).
    Passphrase,
    /// Uniformly random key material expanded with HKDF-SHA256 (version 2).
    KeyMaterial,
}

impl Derivation {
    pub(crate) const fn version(self) -> u8 {
        match self {
            Self::Passphrase => 1,
            Self::KeyMaterial => 2,
        }
    }

    pub(crate) fn from_version(version: u8) -> Option<Self> {
        match version {
            1 => Some(Self::Passphrase),
            2 => Some(Self::KeyMaterial),
            _ => None,
        }
    }
}

/// Master key bytes and their derivation; zeroized on drop.
pub(crate) struct MasterKey {
    bytes: Vec<u8>,
    derivation: Derivation,
}

impl MasterKey {
    pub(crate) fn passphrase(key: &[u8]) -> Self {
        Self {
            bytes: key.to_vec(),
            derivation: Derivation::Passphrase,
        }
    }

    pub(crate) fn key_material(key: &[u8]) -> AppResult<Self> {
        if key.len() < MIN_KEY_MATERIAL {
            return Err(AppError::new(
                ErrorCode::InvalidInput,
                "key material must be at least 32 bytes",
            ));
        }
        Ok(Self {
            bytes: key.to_vec(),
            derivation: Derivation::KeyMaterial,
        })
    }

    fn message_key(&self, algorithm: Algorithm, salt: &[u8]) -> AppResult<[u8; KEY_LEN]> {
        let mut key = [0u8; KEY_LEN];
        match self.derivation {
            Derivation::Passphrase => {
                pbkdf2::pbkdf2_hmac::<Sha256>(&self.bytes, salt, PBKDF2_ITERATIONS, &mut key);
            }
            Derivation::KeyMaterial => {
                let info = format!("kit.encryption.v2/{}", algorithm.name());
                Hkdf::<Sha256>::new(Some(salt), &self.bytes)
                    .expand(info.as_bytes(), &mut key)
                    .map_err(|_| AppError::new(ErrorCode::Internal, "key derivation failed"))?;
            }
        }
        Ok(key)
    }

    /// Seal plaintext with the given salt and nonce into an encoded envelope.
    pub(crate) fn seal(
        &self,
        algorithm: Algorithm,
        plaintext: &[u8],
        salt: &[u8; SALT_SIZE],
        nonce: &[u8; NONCE_SIZE],
    ) -> AppResult<String> {
        let aad = crate::envelope::associated_data(self.derivation, algorithm, salt, nonce);
        let payload = Payload {
            msg: plaintext,
            aad: &aad,
        };
        let mut key = self.message_key(algorithm, salt)?;
        let sealed = match algorithm {
            Algorithm::AesGcm => Aes256Gcm::new((&key[..]).into()).encrypt(nonce.into(), payload),
            Algorithm::ChaCha20Poly1305 => {
                ChaCha20Poly1305::new((&key[..]).into()).encrypt(nonce.into(), payload)
            }
        };
        key.zeroize();
        let ciphertext =
            sealed.map_err(|_| AppError::new(ErrorCode::Internal, "encryption failed"))?;
        Ok(Envelope::encode(
            self.derivation,
            algorithm,
            salt,
            nonce,
            &ciphertext,
        ))
    }

    /// Open an encoded envelope sealed for this key's derivation and the given algorithm.
    pub(crate) fn open(&self, algorithm: Algorithm, ciphertext: &str) -> AppResult<Vec<u8>> {
        let envelope = Envelope::decode(ciphertext)?;
        if envelope.derivation()? != self.derivation {
            return Err(AppError::new(
                ErrorCode::InvalidFormat,
                "ciphertext key derivation does not match encryptor",
            ));
        }
        if envelope.algorithm()? != algorithm {
            return Err(AppError::new(
                ErrorCode::InvalidFormat,
                "ciphertext algorithm does not match encryptor",
            ));
        }
        let payload = Payload {
            msg: envelope.ciphertext(),
            aad: envelope.associated_data(),
        };
        let mut key = self.message_key(algorithm, envelope.salt())?;
        let opened = match algorithm {
            Algorithm::AesGcm => {
                Aes256Gcm::new((&key[..]).into()).decrypt(envelope.nonce().into(), payload)
            }
            Algorithm::ChaCha20Poly1305 => {
                ChaCha20Poly1305::new((&key[..]).into()).decrypt(envelope.nonce().into(), payload)
            }
        };
        key.zeroize();
        opened.map_err(|_| {
            AppError::new(ErrorCode::InvalidFormat, "ciphertext authentication failed")
        })
    }
}

impl Drop for MasterKey {
    fn drop(&mut self) {
        self.bytes.zeroize();
    }
}
