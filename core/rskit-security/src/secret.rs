//! Shared secret handling primitives.

pub use rskit_util::SecretString;
use subtle::{Choice, ConstantTimeEq};

/// Compare byte slices without returning early on content or length mismatch.
///
/// The loop runs for the longer input length and includes the length difference
/// in the result, so callers should still treat secret lengths as externally
/// observable and prefer fixed-length encodings for sensitive tokens.
#[must_use]
pub fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    let mut equal = Choice::from(u8::from(left.len() == right.len()));
    for index in 0..left.len().max(right.len()) {
        let left_byte = left.get(index).copied().unwrap_or_default();
        let right_byte = right.get(index).copied().unwrap_or_default();
        equal &= left_byte.ct_eq(&right_byte);
    }
    equal.into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secret_string_debug_and_display_are_redacted() {
        let secret = SecretString::new("super-secret");

        assert_eq!(format!("{secret}"), "***");
        assert_eq!(format!("{secret:?}"), "SecretString(***)");
    }

    #[test]
    fn constant_time_compare_matches_equal_inputs() {
        assert!(constant_time_eq(b"token", b"token"));
        assert!(!constant_time_eq(b"token", b"other"));
        assert!(!constant_time_eq(b"token", b"token-longer"));
    }
}
