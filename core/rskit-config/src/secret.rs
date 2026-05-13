//! Secret string type that masks its value in `Display`, `Debug`, and serialization.

use std::fmt;

const SECRET_MASK: &str = "***";

/// A string wrapper that masks its value in [`fmt::Display`], [`fmt::Debug`], and JSON
/// serialisation to prevent accidental secret exposure in logs or config dumps.
///
/// Use [`SecretString::expose()`] to access the plaintext.
///
/// # Examples
///
/// ```
/// use rskit_config::SecretString;
///
/// let s = SecretString::new("hunter2");
/// assert_eq!(format!("{s}"), "***");
/// assert_eq!(s.expose(), "hunter2");
/// ```
#[derive(Clone, Default)]
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
}

impl SecretString {
    /// Return the plaintext value.
    pub fn expose(&self) -> &str {
        &self.value
    }

    /// Return `true` if the underlying value is empty.
    pub fn is_empty(&self) -> bool {
        self.value.is_empty()
    }

    /// Return the length of the underlying value.
    pub fn len(&self) -> usize {
        self.value.len()
    }
}

impl zeroize::Zeroize for SecretString {
    fn zeroize(&mut self) {
        self.value.zeroize();
    }
}

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

impl PartialEq for SecretString {
    fn eq(&self, other: &Self) -> bool {
        self.value == other.value
    }
}

impl Eq for SecretString {}

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
}
