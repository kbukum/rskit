//! RFC 3339 UTC date/time formatting and parsing.

use super::civil::{CivilDate, CivilDateTime, datetime_from_epoch_secs, epoch_secs_from_datetime};
use std::time::{SystemTime, UNIX_EPOCH};

const RFC3339_MIN_YEAR: i64 = 0;
const RFC3339_MAX_YEAR: i64 = 9999;

/// Formats a Unix timestamp (whole seconds) as a UTC RFC 3339 string (`YYYY-MM-DDTHH:MM:SSZ`).
///
/// Uses std-only integer arithmetic, so it is portable across platforms.
///
/// # Examples
///
/// ```
/// use rskit_util::time::format_rfc3339;
/// assert_eq!(format_rfc3339(0), Some("1970-01-01T00:00:00Z".to_owned()));
/// assert_eq!(format_rfc3339(1_700_000_000), Some("2023-11-14T22:13:20Z".to_owned()));
/// ```
#[must_use]
pub fn format_rfc3339(epoch_secs: i64) -> Option<String> {
    format_rfc3339_datetime(datetime_from_epoch_secs(epoch_secs))
}

/// Formats a Unix timestamp (whole seconds) as compact UTC `YYYYMMDD-HHMMSS`.
///
/// This is intended for filenames
/// and identifiers that need stable UTC ordering without RFC 3339 punctuation.
#[must_use]
pub fn format_compact_utc(epoch_secs: i64) -> Option<String> {
    let datetime = datetime_from_epoch_secs(epoch_secs);
    (datetime.is_valid() && is_rfc3339_year(datetime.date.year)).then(|| {
        format!(
            "{:04}{:02}{:02}-{:02}{:02}{:02}",
            datetime.date.year,
            datetime.date.month,
            datetime.date.day,
            datetime.hour,
            datetime.minute,
            datetime.second
        )
    })
}

/// Formats a valid UTC civil date/time as `YYYY-MM-DDTHH:MM:SSZ`.
///
/// Returns `None` when the provided date/time fields are invalid
/// or the year is outside RFC 3339's four-digit range.
#[must_use]
pub fn format_rfc3339_datetime(datetime: CivilDateTime) -> Option<String> {
    (datetime.is_valid() && is_rfc3339_year(datetime.date.year)).then(|| format_datetime(datetime))
}

/// Parses a canonical UTC RFC 3339 timestamp into Unix epoch seconds.
///
/// Accepted input is `YYYY-MM-DDTHH:MM:SSZ` with whole-second precision. Lowercase `t`
/// and `z` are also accepted. Offsets, fractional seconds, and leap seconds are rejected.
#[must_use]
pub fn parse_rfc3339_utc(s: &str) -> Option<i64> {
    epoch_secs_from_datetime(parse_rfc3339_utc_datetime(s)?)
}

/// Parses a canonical UTC RFC 3339 timestamp into a UTC civil date/time.
///
/// Accepted input is `YYYY-MM-DDTHH:MM:SSZ` with whole-second precision. Lowercase `t`
/// and `z` are also accepted. Offsets, fractional seconds, and leap seconds are rejected.
#[must_use]
pub fn parse_rfc3339_utc_datetime(s: &str) -> Option<CivilDateTime> {
    let bytes = s.as_bytes();
    if bytes.len() != 20
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || !matches!(bytes[10], b'T' | b't')
        || bytes[13] != b':'
        || bytes[16] != b':'
        || !matches!(bytes[19], b'Z' | b'z')
    {
        return None;
    }

    let year = parse_4(bytes, 0)?;
    let date = CivilDate::new(year, parse_2(bytes, 5)?, parse_2(bytes, 8)?)?;
    CivilDateTime::new(
        date,
        parse_2(bytes, 11)?,
        parse_2(bytes, 14)?,
        parse_2(bytes, 17)?,
    )
}

/// Returns the current Unix epoch time in whole seconds,
/// or `None` if the system clock is set before the Unix epoch or exceeds `i64`.
#[must_use]
pub fn now_epoch_secs() -> Option<i64> {
    let secs = SystemTime::now().duration_since(UNIX_EPOCH).ok()?.as_secs();
    i64::try_from(secs).ok()
}

/// Returns the current UTC civil date/time, or `None` if the system clock is set before the Unix epoch
/// or exceeds `i64`.
#[must_use]
pub fn now_utc() -> Option<CivilDateTime> {
    now_epoch_secs().map(datetime_from_epoch_secs)
}

/// Returns the current wall-clock time as a UTC RFC 3339 string,
/// or `None` if the system clock is set before the Unix epoch or exceeds `i64`.
///
/// # Examples
///
/// ```
/// use rskit_util::time::now_rfc3339;
/// // Format is `YYYY-MM-DDTHH:MM:SSZ`.
/// assert!(now_rfc3339().is_some_and(|s| s.ends_with('Z') && s.contains('T')));
/// ```
#[must_use]
pub fn now_rfc3339() -> Option<String> {
    now_epoch_secs().and_then(format_rfc3339)
}

fn parse_2(bytes: &[u8], start: usize) -> Option<i64> {
    let tens = i64::from(digit(bytes[start])?);
    let ones = i64::from(digit(bytes[start + 1])?);
    Some(tens * 10 + ones)
}

fn parse_4(bytes: &[u8], start: usize) -> Option<i64> {
    let thousands = i64::from(digit(bytes[start])?);
    let hundreds = i64::from(digit(bytes[start + 1])?);
    let tens = i64::from(digit(bytes[start + 2])?);
    let ones = i64::from(digit(bytes[start + 3])?);
    Some(thousands * 1_000 + hundreds * 100 + tens * 10 + ones)
}

fn digit(byte: u8) -> Option<u8> {
    byte.is_ascii_digit().then_some(byte - b'0')
}

fn format_datetime(datetime: CivilDateTime) -> String {
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        datetime.date.year,
        datetime.date.month,
        datetime.date.day,
        datetime.hour,
        datetime.minute,
        datetime.second
    )
}

const fn is_rfc3339_year(year: i64) -> bool {
    year >= RFC3339_MIN_YEAR && year <= RFC3339_MAX_YEAR
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn formats_rfc3339() {
        assert_eq!(format_rfc3339(0), Some("1970-01-01T00:00:00Z".to_owned()));
        assert_eq!(
            format_rfc3339(86_399),
            Some("1970-01-01T23:59:59Z".to_owned())
        );
        assert_eq!(
            format_rfc3339(1_700_000_000),
            Some("2023-11-14T22:13:20Z".to_owned())
        );
        assert_eq!(
            format_rfc3339(951_782_400),
            Some("2000-02-29T00:00:00Z".to_owned())
        );
        assert_eq!(format_rfc3339(-1), Some("1969-12-31T23:59:59Z".to_owned()));
    }

    #[test]
    fn formats_compact_utc() {
        assert_eq!(format_compact_utc(0), Some("19700101-000000".to_owned()));
        assert_eq!(
            format_compact_utc(86_400),
            Some("19700102-000000".to_owned())
        );
        assert_eq!(
            format_compact_utc(1_700_000_000),
            Some("20231114-221320".to_owned())
        );
    }

    #[test]
    fn formats_valid_datetime() {
        let datetime = CivilDateTime {
            date: CivilDate {
                year: 2024,
                month: 2,
                day: 29,
            },
            hour: 12,
            minute: 34,
            second: 56,
        };

        assert_eq!(
            format_rfc3339_datetime(datetime),
            Some("2024-02-29T12:34:56Z".to_owned())
        );
    }

    #[test]
    fn formats_rfc3339_boundary_years() {
        assert_eq!(
            format_rfc3339_datetime(CivilDateTime {
                date: CivilDate {
                    year: RFC3339_MIN_YEAR,
                    month: 1,
                    day: 1,
                },
                hour: 0,
                minute: 0,
                second: 0,
            }),
            Some("0000-01-01T00:00:00Z".to_owned())
        );
        assert_eq!(
            format_rfc3339_datetime(CivilDateTime {
                date: CivilDate {
                    year: RFC3339_MAX_YEAR,
                    month: 12,
                    day: 31,
                },
                hour: 23,
                minute: 59,
                second: 59,
            }),
            Some("9999-12-31T23:59:59Z".to_owned())
        );
    }

    #[test]
    fn rejects_invalid_datetime_formatting() {
        assert_eq!(
            format_rfc3339_datetime(CivilDateTime {
                date: CivilDate {
                    year: 2023,
                    month: 2,
                    day: 29,
                },
                hour: 0,
                minute: 0,
                second: 0,
            }),
            None
        );
        assert_eq!(
            format_rfc3339_datetime(CivilDateTime {
                date: CivilDate {
                    year: 10_000,
                    month: 1,
                    day: 1,
                },
                hour: 0,
                minute: 0,
                second: 0,
            }),
            None
        );
        assert_eq!(
            format_rfc3339_datetime(CivilDateTime {
                date: CivilDate {
                    year: -1,
                    month: 1,
                    day: 1,
                },
                hour: 0,
                minute: 0,
                second: 0,
            }),
            None
        );
    }

    #[test]
    fn parses_canonical_utc_rfc3339() {
        assert_eq!(parse_rfc3339_utc("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(parse_rfc3339_utc("1969-12-31T23:59:59Z"), Some(-1));
        assert_eq!(
            parse_rfc3339_utc("2023-11-14T22:13:20Z"),
            Some(1_700_000_000)
        );
        assert_eq!(
            parse_rfc3339_utc_datetime("2024-02-29t12:34:56z"),
            CivilDateTime::new(
                CivilDate {
                    year: 2024,
                    month: 2,
                    day: 29,
                },
                12,
                34,
                56,
            )
        );
    }

    #[test]
    fn rejects_non_canonical_or_invalid_rfc3339() {
        assert_eq!(parse_rfc3339_utc("2024-02-29T12:34:56+00:00"), None);
        assert_eq!(parse_rfc3339_utc("2024-02-29T12:34:56.000Z"), None);
        assert_eq!(parse_rfc3339_utc("2024-02-29T12:34:60Z"), None);
        assert_eq!(parse_rfc3339_utc("2023-02-29T12:34:56Z"), None);
        assert_eq!(parse_rfc3339_utc("2024-13-01T12:34:56Z"), None);
    }

    #[test]
    fn current_rfc3339_has_expected_shape() {
        let now = now_rfc3339().expect("system clock should be after the epoch");
        assert_eq!(now.len(), "1970-01-01T00:00:00Z".len());
        assert!(now.ends_with('Z'));
        assert!(now.contains('T'));
    }
}
