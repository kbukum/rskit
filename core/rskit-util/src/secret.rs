//! Secret string type that masks its value in `Display`, `Debug`, and serialization.

use std::fmt;

use subtle::ConstantTimeEq;

const SECRET_MASK: &str = "***";

/// A string wrapper that masks its value in [`fmt::Display`], [`fmt::Debug`], and JSON
/// serialisation to prevent accidental secret exposure in logs or config dumps.
///
/// Use [`SecretString::expose()`] to access the plaintext.
///
/// # Security properties
///
/// - **Zeroize on drop:** the backing memory is zeroed when this value is dropped
///   (via [`zeroize::ZeroizeOnDrop`]).
/// - **Constant-time equality:** comparisons use [`subtle::ConstantTimeEq`] to
///   prevent timing side-channel attacks.
/// - **Clone creates a zeroed copy:** cloned instances are independently zeroed
///   on drop.
///
/// # Examples
///
/// ```
/// use rskit_util::SecretString;
///
/// let s = SecretString::new("hunter2");
/// assert_eq!(format!("{s}"), "***");
/// assert_eq!(s.expose(), "hunter2");
/// ```
#[derive(Clone, Default, zeroize::Zeroize, zeroize::ZeroizeOnDrop)]
pub struct SecretString {
    value: String,
}

impl SecretString {
    /// Create a new `SecretString` from a plaintext value.
    pub fn new(value: impl Into<String>) -> Self {
        Self {
            value: value.into(),
        }
    }

    /// Return the plaintext value.
    pub fn expose(&self) -> &str {
        &self.value
    }

    /// Return `true` if the underlying value is empty.
    pub const fn is_empty(&self) -> bool {
        self.value.is_empty()
    }

    /// Return the length of the underlying value.
    pub const fn len(&self) -> usize {
        self.value.len()
    }
}

impl PartialEq for SecretString {
    /// Constant-time comparison to prevent timing side-channel attacks.
    ///
    /// Note: when the two values differ in length the length comparison itself
    /// is **not** constant-time (inherent limitation of [`subtle::ConstantTimeEq`]).
    /// This is acceptable for typical use (comparing secrets derived from the
    /// same config field).
    fn eq(&self, other: &Self) -> bool {
        self.value.as_bytes().ct_eq(other.value.as_bytes()).into()
    }
}

impl Eq for SecretString {}

impl fmt::Display for SecretString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.value.is_empty() {
            Ok(())
        } else {
            f.write_str(SECRET_MASK)
        }
    }
}

impl fmt::Debug for SecretString {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SecretString(***)")
    }
}

impl serde::Serialize for SecretString {
    fn serialize<S: serde::Serializer>(&self, ser: S) -> Result<S::Ok, S::Error> {
        // Always serialize as masked — use expose() for the real value.
        if self.value.is_empty() {
            ser.serialize_str("")
        } else {
            ser.serialize_str(SECRET_MASK)
        }
    }
}

impl<'de> serde::Deserialize<'de> for SecretString {
    fn deserialize<D: serde::Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
        let s = String::deserialize(de)?;
        Ok(Self::new(s))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_masks_value() {
        let s = SecretString::new("password123");
        assert_eq!(format!("{s}"), "***");
    }

    #[test]
    fn display_empty_is_empty() {
        let s = SecretString::new("");
        assert_eq!(format!("{s}"), "");
    }

    #[test]
    fn debug_masks_value() {
        let s = SecretString::new("password123");
        assert_eq!(format!("{s:?}"), "SecretString(***)");
    }

    #[test]
    fn expose_returns_plaintext() {
        let s = SecretString::new("hunter2");
        assert_eq!(s.expose(), "hunter2");
    }

    #[test]
    fn is_empty() {
        assert!(SecretString::default().is_empty());
        assert!(!SecretString::new("x").is_empty());
    }

    #[test]
    fn json_serializes_masked() {
        let s = SecretString::new("secret");
        let json = serde_json::to_string(&s).unwrap();
        assert_eq!(json, r#""***""#);
    }

    #[test]
    fn json_deserializes_plaintext() {
        let s: SecretString = serde_json::from_str(r#""actual_value""#).unwrap();
        assert_eq!(s.expose(), "actual_value");
    }

    #[test]
    fn equality_is_constant_time() {
        let a = SecretString::new("same-value");
        let b = SecretString::new("same-value");
        let c = SecretString::new("different");
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn clone_produces_equal_independent_copy() {
        let a = SecretString::new("cloned-secret");
        let b = a.clone();
        assert_eq!(a.expose(), b.expose());
        assert_eq!(a, b);
    }
}
