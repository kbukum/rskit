//! Stable content hashing with optional domain-separated framing.
//!
//! This module provides one canonical content-hash implementation (BLAKE3)
//! behind a small, allocation-light API so callers depend on the concept of a
//! "content hash" rather than on a specific hashing crate. Digests are rendered
//! as lowercase hexadecimal, giving a stable, comparable identity for cache
//! keys, change detection, and deduplication.
//!
//! [`ContentHasher::update_framed`] applies unambiguous domain separation
//! (`label` `:` `value` `NUL`) so concatenated fields cannot collide — e.g.
//! `("ab", "c")` and `("a", "bc")` hash differently. Prefer it whenever several
//! independent fields are folded into one digest.
//!
//! # Example
//!
//! ```
//! use rskit_util::hash::{ContentHasher, hash_hex};
//!
//! // One-shot digest of a byte slice.
//! let digest = hash_hex(b"hello");
//! assert_eq!(digest.len(), 64);
//!
//! // Incremental, domain-separated digest of several fields.
//! let mut hasher = ContentHasher::new();
//! hasher.update_framed(b"name", b"toven");
//! hasher.update_framed(b"version", b"1");
//! let framed = hasher.finalize_hex();
//! assert_ne!(framed, digest);
//! ```

use blake3::Hasher;

/// Separator written between a field's label and value during framed updates.
const FIELD_SEPARATOR: &[u8] = b":";

/// Terminator written after a framed field, delimiting it from the next.
const FIELD_TERMINATOR: &[u8] = b"\0";

/// Incremental content hasher producing a stable lowercase-hex digest.
///
/// Backed by BLAKE3. Feed bytes with [`update`](Self::update) (raw) or
/// [`update_framed`](Self::update_framed) (domain-separated), then read the
/// digest with [`finalize_hex`](Self::finalize_hex). The hasher may be reused
/// after finalizing; finalizing does not consume it.
#[derive(Clone, Default)]
pub struct ContentHasher {
    inner: Hasher,
}

impl ContentHasher {
    /// Create an empty hasher.
    #[must_use]
    pub fn new() -> Self {
        Self {
            inner: Hasher::new(),
        }
    }

    /// Fold `bytes` into the digest verbatim, without framing.
    ///
    /// Returns `&mut Self` so updates can be chained.
    pub fn update(&mut self, bytes: &[u8]) -> &mut Self {
        self.inner.update(bytes);
        self
    }

    /// Fold a labelled `value` into the digest with unambiguous framing.
    ///
    /// Writes `label`, [`FIELD_SEPARATOR`], `value`, then [`FIELD_TERMINATOR`],
    /// so independently-folded fields cannot alias one another. Returns
    /// `&mut Self` so updates can be chained.
    pub fn update_framed(&mut self, label: &[u8], value: &[u8]) -> &mut Self {
        self.inner.update(label);
        self.inner.update(FIELD_SEPARATOR);
        self.inner.update(value);
        self.inner.update(FIELD_TERMINATOR);
        self
    }

    /// Render the current digest as a lowercase hexadecimal string.
    ///
    /// Does not consume the hasher: further updates may follow and produce a
    /// new digest.
    #[must_use]
    pub fn finalize_hex(&self) -> String {
        self.inner.finalize().to_hex().to_string()
    }
}

/// Return the lowercase-hex content digest of a single byte slice.
#[must_use]
pub fn hash_hex(bytes: &[u8]) -> String {
    let mut hasher = ContentHasher::new();
    hasher.update(bytes);
    hasher.finalize_hex()
}

#[cfg(test)]
mod tests {
    use super::{ContentHasher, hash_hex};

    #[test]
    fn hash_hex_is_stable_and_64_hex_chars() {
        let first = hash_hex(b"toven");
        let second = hash_hex(b"toven");
        assert_eq!(first, second);
        assert_eq!(first.len(), 64);
        assert!(first.bytes().all(|byte| byte.is_ascii_hexdigit()));
    }

    #[test]
    fn distinct_input_yields_distinct_digest() {
        assert_ne!(hash_hex(b"a"), hash_hex(b"b"));
    }

    #[test]
    fn finalize_does_not_consume_the_hasher() {
        let mut hasher = ContentHasher::new();
        hasher.update(b"a");
        let after_a = hasher.finalize_hex();
        hasher.update(b"b");
        let after_ab = hasher.finalize_hex();
        assert_ne!(after_a, after_ab);
    }

    #[test]
    fn framing_prevents_field_boundary_collisions() {
        let mut left = ContentHasher::new();
        left.update_framed(b"ab", b"c");
        let mut right = ContentHasher::new();
        right.update_framed(b"a", b"bc");
        assert_ne!(left.finalize_hex(), right.finalize_hex());
    }

    #[test]
    fn raw_concatenation_would_collide_without_framing() {
        // Demonstrates why framing matters: raw updates of split fields alias.
        let mut left = ContentHasher::new();
        left.update(b"ab").update(b"c");
        let mut right = ContentHasher::new();
        right.update(b"a").update(b"bc");
        assert_eq!(left.finalize_hex(), right.finalize_hex());
    }
}
