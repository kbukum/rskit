//! Minimal domain-free utility crate for the rskit ecosystem.
//!
//! Provides fundamental helper modules for string casing, safe truncation,
//! collection transformation, safe environment variable parsing, duration/byte
//! size parsing, and stateless mathematical exponential backoff.
//!
//! # Modules
//!
//! - [`backoff`]: State-free mathematical backoff calculations with jitter.
//! - [`bytes`]: Formatting and parsing human-readable data sizes.
//! - [`collections`]: Vector grouping, chunking, indexing, and partition helpers.
//! - [`mod@env`]: Safe environment variable parsing with defaults.
//! - [`glob`]: Shell-style `*`/`?` matching over single string segments.
//! - [`hash`]: Content/interop hashing — BLAKE3 and SHA-256.
//! - [`secret`]: Prevent accidental credential leaks in logs/debug outputs.
//! - [`sensitive`]: Matching helpers for names that commonly carry secrets.
//! - [`strings`]: Casing, safe truncation, unique-shorthand resolution, and "did you mean?" suggestions.
//! - [`template`]: Lightweight template engine (`{name}` interpolation).
//! - [`time`]: Duration parsing, UTC date/time conversion, RFC 3339 helpers, and timing wrappers.

#![warn(missing_docs)]

pub mod backoff;
pub mod bytes;
pub mod collections;
pub mod env;
pub mod glob;
pub mod hash;
pub mod secret;
pub mod sensitive;
pub mod strings;
pub mod template;
pub mod time;

pub use secret::SecretString;
pub use sensitive::SecretKeyMatcher;
pub use template::{Placeholder, Template, TemplatePart};

pub(crate) fn parse_decimal_scaled(value: &str, multiplier: u128) -> Option<u128> {
    if value.is_empty() || value.starts_with('-') || value.starts_with('+') {
        return None;
    }

    let (whole, fraction) = value.split_once('.').unwrap_or((value, ""));
    if whole.is_empty() && fraction.is_empty() {
        return None;
    }
    if !whole.bytes().all(|byte| byte.is_ascii_digit())
        || !fraction.bytes().all(|byte| byte.is_ascii_digit())
    {
        return None;
    }

    let whole = if whole.is_empty() {
        0
    } else {
        whole.parse::<u128>().ok()?
    };
    let whole_scaled = whole.checked_mul(multiplier)?;

    if fraction.is_empty() {
        return Some(whole_scaled);
    }

    let scale = 10_u128.checked_pow(u32::try_from(fraction.len()).ok()?)?;
    let fraction = fraction.parse::<u128>().ok()?;
    let fraction_scaled = fraction.checked_mul(multiplier)?.checked_div(scale)?;
    whole_scaled.checked_add(fraction_scaled)
}

#[cfg(test)]
mod tests {
    use super::parse_decimal_scaled;

    #[test]
    fn parse_decimal_scaled_accepts_whole_and_fractional_values() {
        assert_eq!(parse_decimal_scaled("0", 1000), Some(0));
        assert_eq!(parse_decimal_scaled("12", 1000), Some(12_000));
        assert_eq!(parse_decimal_scaled("12.345", 1000), Some(12_345));
        assert_eq!(parse_decimal_scaled(".5", 1000), Some(500));
        assert_eq!(parse_decimal_scaled("1.", 1000), Some(1000));
        assert_eq!(parse_decimal_scaled("1.2345", 1000), Some(1234));
    }

    #[test]
    fn parse_decimal_scaled_rejects_invalid_or_overflowing_values() {
        assert_eq!(parse_decimal_scaled("", 1000), None);
        assert_eq!(parse_decimal_scaled("+1", 1000), None);
        assert_eq!(parse_decimal_scaled("-1", 1000), None);
        assert_eq!(parse_decimal_scaled(".", 1000), None);
        assert_eq!(parse_decimal_scaled("1.2.3", 1000), None);
        assert_eq!(parse_decimal_scaled("1a", 1000), None);
        assert_eq!(parse_decimal_scaled("2", u128::MAX), None);
        assert_eq!(parse_decimal_scaled("1.1", u128::MAX), None);
    }
}
