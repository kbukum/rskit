//! Duration string parsing, formatting, and timing execution wrappers.

use std::time::{Duration, Instant};

/// Runs a synchronous function and returns a tuple containing its return value
/// and the exact execution time.
///
/// # Examples
///
/// ```
/// use rskit_util::time::time_it;
/// let (result, duration) = time_it(|| {
///     // perform some work
///     42
/// });
/// assert_eq!(result, 42);
/// ```
pub fn time_it<F, T>(f: F) -> (T, Duration)
where
    F: FnOnce() -> T,
{
    let start = Instant::now();
    let result = f();
    (result, start.elapsed())
}

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

/// Parses simple duration strings like `"5s"`, `"10m"`, `"1h"` into a `Duration`.
/// Case-insensitive, supports optional space.
///
/// # Examples
///
/// ```
/// use std::time::Duration;
/// use rskit_util::time::parse_duration;
/// assert_eq!(parse_duration("5s"), Some(Duration::from_secs(5)));
/// assert_eq!(parse_duration("10m"), Some(Duration::from_secs(600)));
/// ```
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

fn parse_decimal_scaled(value: &str, multiplier: u128) -> Option<u128> {
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
    let whole_nanos = whole.checked_mul(multiplier)?;

    if fraction.is_empty() {
        return Some(whole_nanos);
    }

    let scale = 10_u128.checked_pow(u32::try_from(fraction.len()).ok()?)?;
    let fraction = fraction.parse::<u128>().ok()?;
    let fraction_nanos = fraction.checked_mul(multiplier)?.checked_div(scale)?;
    whole_nanos.checked_add(fraction_nanos)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_time_it() {
        let (res, elapsed) = time_it(|| 42);
        assert_eq!(res, 42);
        assert!(elapsed <= Duration::from_secs(1));
    }

    #[test]
    fn test_format_duration() {
        assert_eq!(format_duration(Duration::from_hours(2)), "2.00h");
        assert_eq!(format_duration(Duration::from_secs(150)), "2.50m");
        assert_eq!(format_duration(Duration::from_secs(5)), "5.00s");
        assert_eq!(format_duration(Duration::from_millis(250)), "250ms");
    }

    #[test]
    fn test_parse_duration() {
        assert_eq!(parse_duration("5s"), Some(Duration::from_secs(5)));
        assert_eq!(parse_duration("10m"), Some(Duration::from_mins(10)));
        assert_eq!(parse_duration("1.5h"), Some(Duration::from_mins(90)));
        assert_eq!(parse_duration("1.5ms"), Some(Duration::from_micros(1500)));
        assert_eq!(parse_duration("-1s"), None);
        assert_eq!(parse_duration("invalid"), None);
    }
}
