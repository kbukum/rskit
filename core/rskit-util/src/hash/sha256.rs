//! SHA-256 digests for wire-format and interop use cases.
//!
//! The canonical content hash for cache keys, change detection,
//! and deduplication is BLAKE3 (see [`crate::hash::hash_hex`]).
//! SHA-256 lives here only for **interop**: manifests, packs,
//! and protocols that mandate a SHA-256 field on the wire.
//! Digests are rendered as lowercase hexadecimal.
//! Do not reach for this when a stable internal identity is all that's needed — prefer BLAKE3.

use sha2::{Digest, Sha256};
use std::fmt::Write as _;

/// Incremental SHA-256 hasher producing a lowercase-hex digest.
///
/// Feed bytes with [`update`](Self::update),
/// then read the digest with [`finalize_hex`](Self::finalize_hex). Use only for wire-format interop;
/// prefer [`super::ContentHasher`] (BLAKE3) for internal hashing.
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

    /// Consume the hasher and return the lowercase-hex SHA-256 digest.
    #[must_use]
    pub fn finalize_hex(self) -> String {
        let digest = self.inner.finalize();
        let mut out = String::with_capacity(digest.len() * 2);
        for byte in digest {
            let _ = write!(out, "{byte:02x}");
        }
        out
    }
}

/// Return the lowercase-hex SHA-256 digest of a single byte slice.
#[must_use]
pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256Hasher::new();
    hasher.update(bytes);
    hasher.finalize_hex()
}

#[cfg(test)]
mod tests {
    use super::sha256_hex;

    #[test]
    fn sha256_hex_matches_known_vector() {
        // SHA-256("abc") well-known test vector.
        assert_eq!(
            sha256_hex(b"abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
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
}
