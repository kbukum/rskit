//! Versioned ciphertext envelope encoding.

use base64::{Engine, engine::general_purpose::STANDARD};
use rskit_errors::{AppError, AppResult, ErrorCode};

use crate::Algorithm;

/// Random salt length used by PBKDF2-SHA256.
pub(crate) const SALT_SIZE: usize = 16;
/// AEAD nonce length for AES-GCM and ChaCha20-Poly1305.
pub(crate) const NONCE_SIZE: usize = 12;
/// PBKDF2-SHA256 iteration count for passphrase-derived encryption keys.
pub(crate) const PBKDF2_ITERATIONS: u32 = 600_000;
/// Symmetric encryption key length for supported algorithms.
pub(crate) const KEY_LEN: usize = 32;

const VERSION: u8 = 1;
const HEADER_SIZE: usize = 1 + 1 + SALT_SIZE + NONCE_SIZE;
const AEAD_TAG_SIZE: usize = 16;

/// Decoded versioned ciphertext envelope.
pub(crate) struct Envelope {
    data: Vec<u8>,
}

impl Envelope {
    /// Encode a sealed ciphertext envelope as base64.
    pub(crate) fn encode(
        algorithm: Algorithm,
        salt: &[u8; SALT_SIZE],
        nonce: &[u8; NONCE_SIZE],
        ciphertext: &[u8],
    ) -> String {
        let mut data = Vec::with_capacity(HEADER_SIZE + ciphertext.len());
        data.push(VERSION);
        data.push(algorithm.id());
        data.extend_from_slice(salt);
        data.extend_from_slice(nonce);
        data.extend_from_slice(ciphertext);
        STANDARD.encode(data)
    }

    /// Decode and minimally validate a ciphertext envelope.
    pub(crate) fn decode(ciphertext: &str) -> AppResult<Self> {
        let data = STANDARD.decode(ciphertext).map_err(|err| {
            AppError::new(ErrorCode::InvalidFormat, "ciphertext is not valid base64")
                .with_cause(err)
        })?;

        if data.len() < HEADER_SIZE + AEAD_TAG_SIZE {
            return Err(AppError::new(
                ErrorCode::InvalidFormat,
                "ciphertext envelope is too short",
            ));
        }
        if data[0] != VERSION {
            return Err(AppError::new(
                ErrorCode::InvalidFormat,
                "unsupported ciphertext envelope version",
            ));
        }
        if Algorithm::from_id(data[1]).is_none() {
            return Err(AppError::new(
                ErrorCode::InvalidFormat,
                "unsupported ciphertext algorithm",
            ));
        }

        Ok(Self { data })
    }

    /// Return the algorithm declared by this envelope.
    pub(crate) fn algorithm(&self) -> AppResult<Algorithm> {
        Algorithm::from_id(self.data[1]).ok_or_else(|| {
            AppError::new(ErrorCode::InvalidFormat, "unsupported ciphertext algorithm")
        })
    }

    /// Return the bytes authenticated as AEAD associated data.
    pub(crate) fn associated_data(&self) -> &[u8] {
        &self.data[..HEADER_SIZE]
    }

    /// Return the envelope salt.
    pub(crate) fn salt(&self) -> &[u8] {
        &self.data[2..2 + SALT_SIZE]
    }

    /// Return the envelope nonce.
    pub(crate) fn nonce(&self) -> &[u8] {
        &self.data[2 + SALT_SIZE..HEADER_SIZE]
    }

    /// Return the sealed ciphertext body.
    pub(crate) fn ciphertext(&self) -> &[u8] {
        &self.data[HEADER_SIZE..]
    }
}

/// Associated data for a newly generated envelope header.
pub(crate) fn associated_data(
    algorithm: Algorithm,
    salt: &[u8; SALT_SIZE],
    nonce: &[u8; NONCE_SIZE],
) -> [u8; HEADER_SIZE] {
    let mut aad = [0u8; HEADER_SIZE];
    aad[0] = VERSION;
    aad[1] = algorithm.id();
    aad[2..2 + SALT_SIZE].copy_from_slice(salt);
    aad[2 + SALT_SIZE..].copy_from_slice(nonce);
    aad
}
