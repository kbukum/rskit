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
