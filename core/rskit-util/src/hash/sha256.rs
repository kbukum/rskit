//! SHA-256 digests for wire-format and interop use cases.
//!
//! The canonical content hash for cache keys, change detection,
//! and deduplication is BLAKE3 (see [`crate::hash::hash_hex`]).
//! SHA-256 lives here only for **interop**: manifests, packs,
//! and protocols that mandate a SHA-256 field on the wire.
//! Digests are rendered as lowercase hexadecimal.
//! Do not reach for this when a stable internal identity is all that's needed — prefer BLAKE3.

use std::fmt;
use std::io::Read;

use sha2::{Digest as _, Sha256};

/// A SHA-256 digest (32 raw bytes), rendered as lowercase hex on display.
#[derive(Debug, Clone, Copy, Eq, PartialEq, Hash)]
pub struct Sha256Digest([u8; 32]);

impl Sha256Digest {
    /// The raw 32-byte digest.
    #[must_use]
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// The digest as a 64-character lowercase hex string.
    #[must_use]
    pub fn to_hex(&self) -> String {
        self.to_string()
    }
}

impl fmt::Display for Sha256Digest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in &self.0 {
            write!(formatter, "{byte:02x}")?;
        }
        Ok(())
    }
}

/// Incremental SHA-256 hasher producing a [`Sha256Digest`].
///
/// Feed bytes with [`update`](Self::update), then read the digest with
/// [`finalize`](Self::finalize) (or [`finalize_hex`](Self::finalize_hex) for the
/// hex string directly). Use only for wire-format interop; prefer
/// [`super::ContentHasher`] (BLAKE3) for internal hashing.
#[derive(Clone, Default)]
pub struct Sha256Hasher {
    inner: Sha256,
}

impl Sha256Hasher {
    /// Create an empty hasher.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: Sha256::new(),
        }
    }

    /// Feed raw bytes into the digest.
    pub fn update(&mut self, bytes: &[u8]) {
        self.inner.update(bytes);
    }

    /// Consume the hasher and return the [`Sha256Digest`].
    #[must_use]
    pub fn finalize(self) -> Sha256Digest {
        Sha256Digest(self.inner.finalize().into())
    }

    /// Consume the hasher and return the lowercase-hex SHA-256 digest.
    #[must_use]
    pub fn finalize_hex(self) -> String {
        self.finalize().to_hex()
    }
}

/// Compute the SHA-256 [`Sha256Digest`] of an in-memory byte slice.
#[must_use]
pub fn sha256(bytes: &[u8]) -> Sha256Digest {
    let mut hasher = Sha256Hasher::new();
    hasher.update(bytes);
    hasher.finalize()
}

/// Return the lowercase-hex SHA-256 digest of a single byte slice.
#[must_use]
pub fn sha256_hex(bytes: &[u8]) -> String {
    sha256(bytes).to_hex()
}

/// Compute the SHA-256 digest of a stream, reading in bounded chunks so large
/// inputs never need to be fully resident in memory.
///
/// # Errors
/// Propagates any read failure from the underlying reader as [`std::io::Error`].
pub fn sha256_reader<R: Read>(reader: &mut R) -> std::io::Result<Sha256Digest> {
    let mut hasher = Sha256Hasher::new();
    let mut buffer = [0_u8; 8192];
    loop {
        let read = reader.read(&mut buffer)?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Ok(hasher.finalize())
}

#[cfg(test)]
mod tests {
    use super::{sha256, sha256_hex, sha256_reader};

    // Known-answer vectors from FIPS 180-2 / NIST.
    const EMPTY: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
    const ABC: &str = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";

    #[test]
    fn sha256_matches_known_answers() {
        assert_eq!(sha256(b"").to_hex(), EMPTY);
        assert_eq!(sha256(b"abc").to_hex(), ABC);
    }

    #[test]
    fn sha256_hex_matches_known_vector() {
        assert_eq!(sha256_hex(b"abc"), ABC);
    }

    #[test]
    fn sha256_hex_is_64_hex_chars() {
        let digest = sha256_hex(b"toven");
        assert_eq!(digest.len(), 64);
        assert!(
            digest
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        );
    }

    #[test]
    fn display_renders_lowercase_hex() {
        assert_eq!(sha256(b"abc").to_string(), ABC);
    }

    #[test]
    fn reader_matches_the_in_memory_digest_across_chunk_boundaries() {
        // Longer than the internal 8 KiB buffer to exercise multi-chunk hashing.
        let payload = vec![0x5a_u8; 20_000];
        let mut cursor = std::io::Cursor::new(payload.clone());
        assert_eq!(sha256_reader(&mut cursor).unwrap(), sha256(&payload));
    }
}
