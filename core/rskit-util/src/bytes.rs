//! Formatting and parsing human-readable data sizes.

use crate::parse_decimal_scaled;

/// Formats a raw byte count into a human-readable string (e.g. `1048576` -> `"1.00 MB"`).
/// Supported units: B, KB, MB, GB, TB, PB.
///
/// # Examples
///
/// ```
/// use rskit_util::bytes::format_bytes;
/// assert_eq!(format_bytes(1024), "1.00 KB");
/// assert_eq!(format_bytes(1048576), "1.00 MB");
/// ```
pub fn format_bytes(bytes: u64) -> String {
    const UNITS: [&str; 6] = ["B", "KB", "MB", "GB", "TB", "PB"];
    const BASE: u64 = 1024;

    let mut unit = 0;
    let mut divisor = 1_u64;
    while unit + 1 < UNITS.len() && bytes / divisor >= BASE {
        unit += 1;
        divisor = divisor.saturating_mul(BASE);
    }

    if unit == 0 {
        format!("{bytes} B")
    } else {
        let whole = bytes / divisor;
        let remainder = bytes % divisor;
        let mut hundredths =
            ((u128::from(remainder) * 100) + (u128::from(divisor) / 2)) / u128::from(divisor);
        let mut whole = whole;
        if hundredths == 100 {
            whole = whole.saturating_add(1);
            hundredths = 0;
        }
        format!("{whole}.{hundredths:02} {}", UNITS[unit])
    }
}

/// Parses human-readable sizes (e.g. `"5MB"`, `"10KB"`, `"2GB"`) into a raw byte count (`u64`).
/// Case-insensitive, supports optional space, and treats unit-less values as bytes.
///
/// # Examples
///
/// ```
/// use rskit_util::bytes::parse_bytes;
/// assert_eq!(parse_bytes("512"), Some(512));
/// assert_eq!(parse_bytes("1 KB"), Some(1024));
/// assert_eq!(parse_bytes("5mb"), Some(5242880));
/// ```
pub fn parse_bytes(s: &str) -> Option<u64> {
    let s = s.trim().to_lowercase();
    let (num_part, unit_part) = s.split_at(s.find(|c: char| c.is_alphabetic()).unwrap_or(s.len()));
    let unit = unit_part.trim();

    let multiplier = match unit {
        "b" | "" => 1_u128,
        "kb" | "k" => 1024_u128,
        "mb" | "m" => 1024_u128.pow(2),
        "gb" | "g" => 1024_u128.pow(3),
        "tb" | "t" => 1024_u128.pow(4),
        "pb" | "p" => 1024_u128.pow(5),
        _ => return None,
    };

    parse_decimal_scaled(num_part.trim(), multiplier).and_then(|bytes| u64::try_from(bytes).ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_bytes() {
        assert_eq!(format_bytes(0), "0 B");
        assert_eq!(format_bytes(512), "512 B");
        assert_eq!(format_bytes(1024), "1.00 KB");
        assert_eq!(format_bytes(1024 * 1024 * 5), "5.00 MB");
        assert_eq!(format_bytes(1024 * 1024 * 1024 * 2), "2.00 GB");
    }

    #[test]
    fn test_parse_bytes() {
        assert_eq!(parse_bytes("512"), Some(512));
        assert_eq!(parse_bytes("512b"), Some(512));
        assert_eq!(parse_bytes("1 KB"), Some(1024));
        assert_eq!(parse_bytes("5mb"), Some(5_242_880));
        assert_eq!(parse_bytes("2gb"), Some(2_147_483_648));
        assert_eq!(parse_bytes("1.5 KB"), Some(1536));
        assert_eq!(parse_bytes("-1 KB"), None);
        assert_eq!(parse_bytes("18446744073709551616"), None);
        assert_eq!(parse_bytes("invalid"), None);
    }
}
