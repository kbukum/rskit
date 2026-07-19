//! Shared secret handling primitives.

pub use rskit_util::SecretString;
use subtle::ConstantTimeEq;

/// Compare equal-length byte slices in constant time.
///
/// Length mismatches return `false`; callers should still treat secret lengths as externally observable
/// and prefer fixed-length encodings for sensitive tokens.
#[must_use]
pub fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    left.ct_eq(right).into()
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
