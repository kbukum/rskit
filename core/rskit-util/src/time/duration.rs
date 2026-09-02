//! Duration string parsing and formatting.

use crate::parse_decimal_scaled;
use std::time::Duration;

/// Formats a `Duration` into a human-readable string.
///
/// # Examples
///
/// ```
/// use std::time::Duration;
/// use rskit_util::time::format_duration;
/// assert_eq!(format_duration(Duration::from_secs(5)), "5.00s");
/// assert_eq!(format_duration(Duration::from_millis(152)), "152ms");
/// ```
#[must_use]
pub fn format_duration(d: Duration) -> String {
    let secs = d.as_secs_f64();
    if secs >= 3600.0 {
        format!("{:.2}h", secs / 3600.0)
    } else if secs >= 60.0 {
        format!("{:.2}m", secs / 60.0)
    } else if secs >= 1.0 {
        format!("{secs:.2}s")
    } else if d.as_micros() >= 1000 {
        format!("{}ms", d.as_millis())
    } else if d.as_nanos() >= 1000 {
        format!("{}μs", d.as_micros())
    } else {
        format!("{}ns", d.as_nanos())
    }
}

/// Formats a `Duration` into a lossless, round-trip-safe string.
///
/// Unlike [`format_duration`], which rounds to two decimals for display, this picks the largest
/// time unit that represents the duration as an exact integer, so
/// [`parse_duration`] reconstructs the original value byte-for-byte. Use it for any duration that
/// is serialized to configuration or a wire contract.
///
/// # Examples
///
/// ```
/// use std::time::Duration;
/// use rskit_util::time::{format_duration_exact, parse_duration};
/// assert_eq!(format_duration_exact(Duration::from_secs(3601)), "3601s");
/// assert_eq!(format_duration_exact(Duration::from_secs(7200)), "2h");
/// assert_eq!(format_duration_exact(Duration::from_millis(1500)), "1500ms");
/// let d = Duration::new(3601, 250);
/// assert_eq!(parse_duration(&format_duration_exact(d)), Some(d));
/// ```
#[must_use]
pub fn format_duration_exact(d: Duration) -> String {
    const NS_PER_US: u128 = 1_000;
    const NS_PER_MS: u128 = 1_000_000;
    const NS_PER_S: u128 = 1_000_000_000;
    const NS_PER_M: u128 = 60 * NS_PER_S;
    const NS_PER_H: u128 = 3_600 * NS_PER_S;

    let nanos = d.as_nanos();
    if nanos == 0 {
        return "0s".to_string();
    }
    if nanos.is_multiple_of(NS_PER_H) {
        format!("{}h", nanos / NS_PER_H)
    } else if nanos.is_multiple_of(NS_PER_M) {
        format!("{}m", nanos / NS_PER_M)
    } else if nanos.is_multiple_of(NS_PER_S) {
        format!("{}s", nanos / NS_PER_S)
    } else if nanos.is_multiple_of(NS_PER_MS) {
        format!("{}ms", nanos / NS_PER_MS)
    } else if nanos.is_multiple_of(NS_PER_US) {
        format!("{}us", nanos / NS_PER_US)
    } else {
        format!("{nanos}ns")
    }
}

/// Parses simple duration strings like `"5s"`, `"10m"`, `"1h"` into a `Duration`. Case-insensitive,
/// supports optional space, and treats unit-less values as seconds.
///
/// # Examples
///
/// ```
/// use std::time::Duration;
/// use rskit_util::time::parse_duration;
/// assert_eq!(parse_duration("5"), Some(Duration::from_secs(5)));
/// assert_eq!(parse_duration("5s"), Some(Duration::from_secs(5)));
/// assert_eq!(parse_duration("10m"), Some(Duration::from_secs(600)));
/// ```
#[must_use]
pub fn parse_duration(s: &str) -> Option<Duration> {
    let s = s.trim().to_lowercase();
    let (num_part, unit_part) = s.split_at(s.find(|c: char| c.is_alphabetic()).unwrap_or(s.len()));
    let unit = unit_part.trim();

    let nanos_per_unit = match unit {
        "ns" => 1_u128,
        "us" | "μs" => 1_000,
        "ms" => 1_000_000,
        "s" | "" => 1_000_000_000,
        "m" | "min" => 60 * 1_000_000_000,
        "h" | "hr" => 3_600 * 1_000_000_000,
        "d" | "day" => 86_400 * 1_000_000_000,
        _ => return None,
    };

    let nanos = parse_decimal_scaled(num_part.trim(), nanos_per_unit)?;
    if nanos > Duration::MAX.as_nanos() {
        return None;
    }
    let secs = nanos / 1_000_000_000;
    let subsec_nanos = nanos % 1_000_000_000;
    Some(Duration::new(
        u64::try_from(secs).ok()?,
        u32::try_from(subsec_nanos).ok()?,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_duration() {
        assert_eq!(format_duration(Duration::from_hours(2)), "2.00h");
        assert_eq!(format_duration(Duration::from_secs(150)), "2.50m");
        assert_eq!(format_duration(Duration::from_secs(5)), "5.00s");
        assert_eq!(format_duration(Duration::from_millis(250)), "250ms");
    }

    #[test]
    fn format_duration_exact_round_trips_losslessly() {
        for d in [
            Duration::ZERO,
            Duration::from_secs(3601),
            Duration::from_hours(2),
            Duration::from_secs(150),
            Duration::from_millis(1500),
            Duration::from_micros(1_500_001),
            Duration::new(3601, 250),
            Duration::from_nanos(1),
        ] {
            let text = format_duration_exact(d);
            assert_eq!(
                parse_duration(&text),
                Some(d),
                "round trip failed for {text}"
            );
        }
        assert_eq!(format_duration_exact(Duration::from_secs(3601)), "3601s");
        assert_eq!(format_duration_exact(Duration::from_hours(2)), "2h");
        assert_eq!(format_duration_exact(Duration::ZERO), "0s");
    }

    #[test]
    fn test_parse_duration() {
        assert_eq!(parse_duration("5"), Some(Duration::from_secs(5)));
        assert_eq!(parse_duration("5s"), Some(Duration::from_secs(5)));
        assert_eq!(parse_duration("10m"), Some(Duration::from_mins(10)));
        assert_eq!(parse_duration("1.5h"), Some(Duration::from_mins(90)));
        assert_eq!(parse_duration("1.5ms"), Some(Duration::from_micros(1500)));
        assert_eq!(parse_duration("-1s"), None);
        assert_eq!(parse_duration("invalid"), None);
    }
}
