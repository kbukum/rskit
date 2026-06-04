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
//! - [`secret`]: Prevent accidental credential leaks in logs/debug outputs.
//! - [`sensitive`]: Matching helpers for names that commonly carry secrets.
//! - [`strings`]: Zero-alloc/low-alloc casing and safe truncation.
//! - [`template`]: Lightweight template engine (`{name}` interpolation).
//! - [`time`]: Duration parsing, UTC date/time conversion, RFC 3339 helpers, and timing wrappers.

#![warn(missing_docs)]

pub mod backoff;
pub mod bytes;
pub mod collections;
pub mod env;
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
