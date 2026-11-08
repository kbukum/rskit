//! Versioned ciphertext envelope encoding.

use base64::{Engine, engine::general_purpose::STANDARD};
use rskit_errors::{AppError, AppResult, ErrorCode};

use crate::Algorithm;
use crate::key::Derivation;

/// Random salt length used by key derivation.
pub(crate) const SALT_SIZE: usize = 16;
/// AEAD nonce length for AES-GCM and ChaCha20-Poly1305.
pub(crate) const NONCE_SIZE: usize = 12;
/// PBKDF2-SHA256 iteration count for passphrase-derived encryption keys.
pub(crate) const PBKDF2_ITERATIONS: u32 = 600_000;
/// Symmetric encryption key length for supported algorithms.
pub(crate) const KEY_LEN: usize = 32;

const HEADER_SIZE: usize = 1 + 1 + SALT_SIZE + NONCE_SIZE;
const AEAD_TAG_SIZE: usize = 16;

/// Decoded versioned ciphertext envelope.
pub(crate) struct Envelope {
    header: [u8; HEADER_SIZE],
    derivation: Derivation,
    algorithm: Algorithm,
    salt: [u8; SALT_SIZE],
    nonce: [u8; NONCE_SIZE],
    ciphertext: Vec<u8>,
}

impl Envelope {
    /// Encode a sealed ciphertext envelope as base64.
    pub(crate) fn encode(
        derivation: Derivation,
        algorithm: Algorithm,
        salt: &[u8; SALT_SIZE],
        nonce: &[u8; NONCE_SIZE],
        ciphertext: &[u8],
    ) -> String {
        let mut data = Vec::with_capacity(HEADER_SIZE + ciphertext.len());
        data.extend_from_slice(&associated_data(derivation, algorithm, salt, nonce));
        data.extend_from_slice(ciphertext);
        STANDARD.encode(data)
    }

    /// Decode and validate a ciphertext envelope header.
    pub(crate) fn decode(ciphertext: &str) -> AppResult<Self> {
        let data = STANDARD.decode(ciphertext).map_err(|err| {
            AppError::new(ErrorCode::InvalidFormat, "ciphertext is not valid base64")
                .with_cause(err)
        })?;

        let too_short =
            || AppError::new(ErrorCode::InvalidFormat, "ciphertext envelope is too short");
        if data.len() < HEADER_SIZE + AEAD_TAG_SIZE {
            return Err(too_short());
        }
        let (header, body) = data
            .split_first_chunk::<HEADER_SIZE>()
            .ok_or_else(too_short)?;
        let [version, algorithm_id, rest @ ..] = header;
        let (salt, nonce) = rest
            .split_first_chunk::<SALT_SIZE>()
            .ok_or_else(too_short)?;
        let nonce: &[u8; NONCE_SIZE] = nonce.try_into().map_err(|_| too_short())?;

        let derivation = Derivation::from_version(*version).ok_or_else(|| {
            AppError::new(
                ErrorCode::InvalidFormat,
                "unsupported ciphertext envelope version",
            )
        })?;
        let algorithm = Algorithm::from_id(*algorithm_id).ok_or_else(|| {
            AppError::new(ErrorCode::InvalidFormat, "unsupported ciphertext algorithm")
        })?;

        Ok(Self {
            header: *header,
            derivation,
            algorithm,
            salt: *salt,
            nonce: *nonce,
            ciphertext: body.to_vec(),
        })
    }

    /// Return the key derivation declared by this envelope's version.
    pub(crate) const fn derivation(&self) -> Derivation {
        self.derivation
    }

    /// Return the algorithm declared by this envelope.
    pub(crate) const fn algorithm(&self) -> Algorithm {
        self.algorithm
    }

    /// Return the bytes authenticated as AEAD associated data.
    pub(crate) const fn associated_data(&self) -> &[u8; HEADER_SIZE] {
        &self.header
    }

    /// Return the envelope salt.
    pub(crate) const fn salt(&self) -> &[u8; SALT_SIZE] {
        &self.salt
    }

    /// Return the envelope nonce.
    pub(crate) const fn nonce(&self) -> &[u8; NONCE_SIZE] {
        &self.nonce
    }

    /// Return the sealed ciphertext body.
    pub(crate) fn ciphertext(&self) -> &[u8] {
        &self.ciphertext
    }
}

/// Associated data for a newly generated envelope header.
pub(crate) fn associated_data(
    derivation: Derivation,
    algorithm: Algorithm,
    salt: &[u8; SALT_SIZE],
    nonce: &[u8; NONCE_SIZE],
) -> [u8; HEADER_SIZE] {
    let mut aad = [0u8; HEADER_SIZE];
    aad[0] = derivation.version();
    aad[1] = algorithm.id();
    aad[2..2 + SALT_SIZE].copy_from_slice(salt);
    aad[2 + SALT_SIZE..].copy_from_slice(nonce);
    aad
}
