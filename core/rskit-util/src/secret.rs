//! Secret string type that masks its value in display, debug, and serialization.

use std::fmt;

use serde::{Deserialize, Deserializer, Serialize, Serializer};
use subtle::ConstantTimeEq;

const SECRET_MASK: &str = "***";

/// A string wrapper that prevents accidental secret exposure in logs or config dumps.
///
/// Use [`SecretString::expose`] to access the plaintext intentionally.
#[derive(Clone, Default, zeroize::Zeroize, zeroize::ZeroizeOnDrop)]
pub struct SecretString {
    value: String,
}

impl SecretString {
    /// Create a new secret from a plaintext value.
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

impl Serialize for SecretString {
    fn serialize<S: Serializer>(&self, ser: S) -> Result<S::Ok, S::Error> {
        if self.value.is_empty() {
            ser.serialize_str("")
        } else {
            ser.serialize_str(SECRET_MASK)
        }
    }
}

impl<'de> Deserialize<'de> for SecretString {
    fn deserialize<D: Deserializer<'de>>(de: D) -> Result<Self, D::Error> {
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
}
