//! RFC 2822 / RFC 5322 date-time formatting and parsing (the "internet message"
//! date format).
//!
//! This is the textual date first defined for email in RFC 822 and carried
//! forward by RFC 2822 and RFC 5322 — `Wdy, DD Mon YYYY HH:MM:SS ±ZZZZ`, e.g.
//! `Sun, 06 Nov 1994 08:49:37 -0500`. It is protocol-neutral: email headers,
//! log lines, and many APIs all use it. HTTP's `Date`/`Retry-After` use a close
//! IMF-fixdate variant (`... GMT`) that this parser accepts, but the formatter
//! always emits the RFC 2822 numeric `+0000` zone rather than the literal `GMT`
//! an HTTP header requires — do not emit this output directly into an HTTP
//! header. The codec knows nothing about any particular transport.
//!
//! Parsing follows RFC 2822 §3.3 and its obsolete forms (§4.3):
//!
//! - The leading day-of-week is optional; when present it must be a valid
//!   `Mon`..`Sun` name that agrees with the date, otherwise the input is
//!   rejected.
//! - Seconds are optional (`HH:MM` is accepted and read as `:00`). Each of the
//!   hour, minute, and second fields is exactly two digits; the day is one or
//!   two digits. A leap-second value (`:60`) is rejected, matching the RFC 3339
//!   sibling.
//! - The zone is a numeric offset (`+HHMM` / `-HHMM`) or a named zone. Numeric
//!   offsets and the North-American named zones (`UT`/`GMT`, `EST`/`EDT`,
//!   `CST`/`CDT`, `MST`/`MDT`, `PST`/`PDT`) are honored; every value is
//!   normalized to UTC. Obsolete single-letter military zones are treated as
//!   `-0000` (unknown offset ⇒ UTC), matching RFC 2822's correction of their
//!   historically reversed sign.
//! - Obsolete two- and three-digit years are expanded per RFC 2822: `00..=49` ⇒
//!   `2000..=2049`, `50..=99` ⇒ `1950..=1999`, and a three-digit `NNN` ⇒
//!   `1900 + NNN`.
//!
//! Formatting always emits the canonical RFC 2822 form with a numeric `+0000`
//! zone. All of this is std-only integer arithmetic (like the RFC 3339 sibling),
//! so it is portable and cheap.

use super::civil::{
    CivilDate, CivilDateTime, datetime_from_epoch_secs, days_from_civil, epoch_secs_from_datetime,
};

/// Three-letter English month abbreviations, indexed by `month - 1`.
const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// Three-letter English day-of-week abbreviations, indexed by
/// `(days_since_epoch + 4) mod 7` (the Unix epoch, 1970-01-01, was a Thursday).
const WEEKDAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];

/// Inclusive four-digit year range the canonical output can render.
const RFC2822_MIN_YEAR: i64 = 0;
const RFC2822_MAX_YEAR: i64 = 9999;

const SECONDS_PER_HOUR: i64 = 3_600;

/// Formats a Unix timestamp (whole seconds) as a canonical RFC 2822 date-time
/// (`Wdy, DD Mon YYYY HH:MM:SS +0000`).
///
/// The zone is always the numeric UTC offset `+0000`. Returns `None` when the
/// instant falls outside the four-digit-year range the canonical form renders.
///
/// # Examples
///
/// ```
/// use rskit_util::time::format_rfc2822;
/// assert_eq!(
///     format_rfc2822(784_130_977),
///     Some("Sun, 06 Nov 1994 14:09:37 +0000".to_owned())
/// );
/// ```
#[must_use]
pub fn format_rfc2822(epoch_secs: i64) -> Option<String> {
    format_rfc2822_datetime(datetime_from_epoch_secs(epoch_secs))
}

/// Formats a valid UTC civil date/time as a canonical RFC 2822 date-time
/// (`Wdy, DD Mon YYYY HH:MM:SS +0000`).
///
/// Returns `None` when the fields are invalid or the year is outside the
/// four-digit range the canonical form renders.
#[must_use]
pub fn format_rfc2822_datetime(datetime: CivilDateTime) -> Option<String> {
    if !datetime.is_valid()
        || datetime.date.year < RFC2822_MIN_YEAR
        || datetime.date.year > RFC2822_MAX_YEAR
    {
        return None;
    }
    let days = days_from_civil(datetime.date)?;
    let weekday = WEEKDAYS[usize::try_from((days + 4).rem_euclid(7)).ok()?];
    let month = MONTHS[usize::try_from(datetime.date.month - 1).ok()?];
    Some(format!(
        "{weekday}, {:02} {month} {:04} {:02}:{:02}:{:02} +0000",
        datetime.date.day, datetime.date.year, datetime.hour, datetime.minute, datetime.second,
    ))
}

/// Parses an RFC 2822 date-time into Unix epoch seconds (UTC).
///
/// Accepts the canonical form and RFC 2822's obsolete variants (optional
/// day-of-week and seconds, numeric or named zones, two/three-digit years). The
/// zone is applied so the result is always a true UTC instant. Returns `None`
/// for input that is not a well-formed RFC 2822 date-time or whose fields are
/// out of range.
///
/// # Examples
///
/// ```
/// use rskit_util::time::parse_rfc2822;
/// // Local time with an explicit offset normalizes to UTC.
/// assert_eq!(
///     parse_rfc2822("Sun, 06 Nov 1994 08:49:37 -0500"),
///     Some(784_129_777),
/// );
/// assert_eq!(parse_rfc2822("not a date"), None);
/// ```
#[must_use]
pub fn parse_rfc2822(s: &str) -> Option<i64> {
    let trimmed = s.trim();
    // The leading day-of-week is optional; when present RFC 2822 requires a
    // valid `Mon`..`Sun` name, so it is captured here and cross-checked against
    // the parsed date below.
    let (weekday, rest) = match trimmed.split_once(',') {
        Some((weekday, tail)) => (Some(weekday.trim()), tail),
        None => (None, trimmed),
    };

    let tokens: [&str; 5] = collect_five(rest)?;
    let [day, month, year, time, zone] = tokens;
    let date = CivilDate::new(
        normalize_year(year)?,
        month_from_name(month)?,
        // RFC 2822 day-of-month is one or two digits.
        parse_uint_bounded(day, 1, 2)?,
    )?;
    if let Some(name) = weekday {
        // 1970-01-01 (day 0) was a Thursday, index 4 in `WEEKDAYS`. Reduce the
        // day count modulo 7 before adding the offset so an extreme year cannot
        // overflow `i64` on the addition.
        let expected = (days_from_civil(date)?.rem_euclid(7) + 4).rem_euclid(7);
        if weekday_index(name)? != expected {
            return None;
        }
    }
    let (hour, minute, second) = parse_time(time)?;
    let local = CivilDateTime::new(date, hour, minute, second)?;
    // Interpret the wall-clock fields as UTC, then subtract the zone offset to
    // recover the true UTC instant the local time denotes. `checked_sub` guards
    // the `i64` epoch boundary so an extreme date plus a nonzero zone yields
    // `None` rather than panicking or wrapping.
    epoch_secs_from_datetime(local)?.checked_sub(zone_offset_seconds(zone)?)
}

/// Parses an RFC 2822 date-time into a UTC-normalized civil date/time.
///
/// Equivalent to [`parse_rfc2822`] followed by a civil conversion, so the
/// returned fields are always in UTC regardless of the input zone.
#[must_use]
pub fn parse_rfc2822_datetime(s: &str) -> Option<CivilDateTime> {
    Some(datetime_from_epoch_secs(parse_rfc2822(s)?))
}

/// Collects exactly five whitespace-separated tokens (`day month year time
/// zone`), rejecting any other arity.
fn collect_five(s: &str) -> Option<[&str; 5]> {
    let mut fields = s.split_whitespace();
    let tokens = [
        fields.next()?,
        fields.next()?,
        fields.next()?,
        fields.next()?,
        fields.next()?,
    ];
    if fields.next().is_some() {
        return None;
    }
    Some(tokens)
}

/// Parses an `HH:MM[:SS]` token; a missing seconds field defaults to `0`. Each
/// present component is exactly two digits per RFC 2822, so widths such as
/// `8:49` or `08:049` are rejected.
fn parse_time(s: &str) -> Option<(i64, i64, i64)> {
    let mut fields = s.splitn(3, ':');
    let hour = parse_uint_bounded(fields.next()?, 2, 2)?;
    let minute = parse_uint_bounded(fields.next()?, 2, 2)?;
    let second = match fields.next() {
        Some(sec) => parse_uint_bounded(sec, 2, 2)?,
        None => 0,
    };
    Some((hour, minute, second))
}

/// Resolves an RFC 2822 zone token to its offset east of UTC, in seconds.
///
/// Handles numeric `±HHMM` offsets and the named zones RFC 2822 defines;
/// obsolete single-letter military zones collapse to `0` (unknown ⇒ UTC).
fn zone_offset_seconds(token: &str) -> Option<i64> {
    let bytes = token.as_bytes();
    if matches!(bytes.first(), Some(b'+' | b'-')) {
        // A numeric zone is exactly `±HHMM` with ASCII digits. Verifying the
        // four digits are ASCII up front keeps the byte-index slices below on
        // char boundaries, so a non-ASCII token such as `+aéa` returns `None`
        // instead of panicking.
        if bytes.len() != 5 || !bytes[1..].iter().all(u8::is_ascii_digit) {
            return None;
        }
        let hours = parse_uint(&token[1..3])?;
        let minutes = parse_uint(&token[3..5])?;
        if hours >= 24 || minutes >= 60 {
            return None;
        }
        let magnitude = hours * SECONDS_PER_HOUR + minutes * 60;
        return Some(if bytes[0] == b'-' {
            -magnitude
        } else {
            magnitude
        });
    }

    let upper = token.to_ascii_uppercase();
    // Each named zone is listed explicitly for readability even though several
    // share a numeric offset (e.g. EST and CDT are both −0500); collapsing them
    // by offset would obscure which zone maps where.
    #[allow(clippy::match_same_arms)]
    Some(match upper.as_str() {
        "UT" | "GMT" => 0,
        "EST" => -5 * SECONDS_PER_HOUR,
        "EDT" => -4 * SECONDS_PER_HOUR,
        "CST" => -6 * SECONDS_PER_HOUR,
        "CDT" => -5 * SECONDS_PER_HOUR,
        "MST" => -7 * SECONDS_PER_HOUR,
        "MDT" => -6 * SECONDS_PER_HOUR,
        "PST" => -8 * SECONDS_PER_HOUR,
        "PDT" => -7 * SECONDS_PER_HOUR,
        // RFC 2822 §4.3: the obsolete single-letter military zones were
        // historically specified with the wrong sign, so a parser must treat
        // them as `-0000` — an unknown offset, i.e. UTC. `J`/`j` is explicitly
        // excluded from the grammar and is therefore rejected rather than
        // normalized.
        _ if upper.len() == 1 && upper.as_bytes()[0].is_ascii_alphabetic() && upper != "J" => 0,
        _ => return None,
    })
}

/// Expands an RFC 2822 year token: two-digit `00..=49` ⇒ `2000..=2049`,
/// `50..=99` ⇒ `1950..=1999`, three-digit `NNN` ⇒ `1900 + NNN`, and a
/// four-or-more-digit year is taken literally. One-digit years are rejected.
fn normalize_year(token: &str) -> Option<i64> {
    let value = parse_uint(token)?;
    match token.len() {
        2 if value <= 49 => Some(2000 + value),
        2 | 3 => Some(1900 + value),
        len if len >= 4 => Some(value),
        _ => None,
    }
}

/// Maps a three-letter English day-of-week abbreviation (case-insensitive) to
/// its index in `WEEKDAYS` (`Sun` = 0 … `Sat` = 6), rejecting any other token.
fn weekday_index(name: &str) -> Option<i64> {
    WEEKDAYS
        .iter()
        .position(|weekday| weekday.eq_ignore_ascii_case(name))
        .and_then(|index| i64::try_from(index).ok())
}

/// Maps a three-letter English month abbreviation (case-insensitive) to its
/// `1..=12` number.
fn month_from_name(name: &str) -> Option<i64> {
    MONTHS
        .iter()
        .position(|month| month.eq_ignore_ascii_case(name))
        .and_then(|index| i64::try_from(index).ok())
        .map(|index| index + 1)
}

/// Parses a non-empty run of ASCII digits into an `i64`, rejecting any other
/// input (signs, whitespace, overflow).
fn parse_uint(s: &str) -> Option<i64> {
    if s.is_empty() || !s.bytes().all(|byte| byte.is_ascii_digit()) {
        return None;
    }
    s.parse::<i64>().ok()
}

/// Parses a run of ASCII digits whose length is within `min_len..=max_len`,
/// rejecting tokens outside that width so zero-padded forms such as `000006`
/// (day) or `049` (a time component) do not slip past the RFC 2822 grammar.
fn parse_uint_bounded(s: &str, min_len: usize, max_len: usize) -> Option<i64> {
    if s.len() < min_len || s.len() > max_len {
        return None;
    }
    parse_uint(s)
}

#[cfg(test)]
mod tests {
    use super::*;

    // The canonical RFC 9110/2822 example instant `Sun, 06 Nov 1994 08:49:37 GMT`.
    const EXAMPLE_UTC_EPOCH: i64 = 784_111_777;

    #[test]
    fn parses_canonical_utc() {
        assert_eq!(
            parse_rfc2822("Sun, 06 Nov 1994 08:49:37 GMT"),
            Some(EXAMPLE_UTC_EPOCH)
        );
        assert_eq!(
            parse_rfc2822("Sun, 06 Nov 1994 08:49:37 +0000"),
            Some(EXAMPLE_UTC_EPOCH)
        );
    }

    #[test]
    fn applies_numeric_offsets_to_reach_utc() {
        // 08:49:37 -0500 is 13:49:37 UTC — five hours later.
        assert_eq!(
            parse_rfc2822("Sun, 06 Nov 1994 08:49:37 -0500"),
            Some(EXAMPLE_UTC_EPOCH + 5 * SECONDS_PER_HOUR)
        );
        // 08:49:37 +0530 is 03:19:37 UTC — five and a half hours earlier.
        assert_eq!(
            parse_rfc2822("Sun, 06 Nov 1994 08:49:37 +0530"),
            Some(EXAMPLE_UTC_EPOCH - (5 * SECONDS_PER_HOUR + 30 * 60))
        );
    }

    #[test]
    fn named_zones_match_their_numeric_offsets() {
        assert_eq!(
            parse_rfc2822("Sun, 06 Nov 1994 08:49:37 EST"),
            parse_rfc2822("Sun, 06 Nov 1994 08:49:37 -0500")
        );
        assert_eq!(
            parse_rfc2822("Sun, 06 Nov 1994 08:49:37 PDT"),
            parse_rfc2822("Sun, 06 Nov 1994 08:49:37 -0700")
        );
    }

    #[test]
    fn obsolete_military_zone_is_treated_as_utc() {
        // A single-letter military zone collapses to -0000 (UTC) per RFC 2822.
        assert_eq!(
            parse_rfc2822("Sun, 06 Nov 1994 08:49:37 A"),
            Some(EXAMPLE_UTC_EPOCH)
        );
    }

    #[test]
    fn day_of_week_is_optional() {
        assert_eq!(
            parse_rfc2822("06 Nov 1994 08:49:37 GMT"),
            Some(EXAMPLE_UTC_EPOCH)
        );
    }

    #[test]
    fn seconds_are_optional() {
        assert_eq!(
            parse_rfc2822("Sun, 06 Nov 1994 08:49 GMT"),
            Some(EXAMPLE_UTC_EPOCH - 37)
        );
    }

    #[test]
    fn expands_obsolete_short_years() {
        let year = |s: &str| parse_rfc2822_datetime(s).map(|dt| dt.date.year);
        assert_eq!(year("06 Nov 94 00:00:00 GMT"), Some(1994));
        assert_eq!(year("06 Nov 49 00:00:00 GMT"), Some(2049));
        assert_eq!(year("06 Nov 50 00:00:00 GMT"), Some(1950));
        assert_eq!(year("06 Nov 994 00:00:00 GMT"), Some(2894)); // 1900 + 994
    }

    #[test]
    fn parses_the_crates_io_retry_after_instant() {
        // crates.io renders its rate-limit deadline in this exact shape.
        // 2026-08-16 is a Sunday, so the day-of-week agrees with the date.
        assert_eq!(
            parse_rfc2822_datetime("Sun, 16 Aug 2026 14:19:08 GMT"),
            CivilDateTime::new(CivilDate::new(2026, 8, 16).unwrap(), 14, 19, 8)
        );
    }

    #[test]
    fn rejects_a_day_of_week_that_disagrees_with_the_date() {
        // 1994-11-06 is a Sunday; any other named weekday is inconsistent input.
        assert_eq!(parse_rfc2822("Wed, 06 Nov 1994 08:49:37 GMT"), None);
    }

    #[test]
    fn rejects_an_unknown_day_of_week_token() {
        // A non-weekday alphabetic token is not a valid RFC 2822 day-of-week.
        assert_eq!(parse_rfc2822("Banana, 06 Nov 1994 08:49:37 GMT"), None);
    }

    #[test]
    fn rejects_the_excluded_military_zone_j() {
        // RFC 2822 §4.3 excludes `J`/`j` from the military-zone grammar.
        assert_eq!(parse_rfc2822("Sun, 06 Nov 1994 08:49:37 J"), None);
        assert_eq!(parse_rfc2822("Sun, 06 Nov 1994 08:49:37 j"), None);
    }

    #[test]
    fn is_case_insensitive_for_month_and_zone() {
        assert_eq!(
            parse_rfc2822("sun, 06 nov 1994 08:49:37 gmt"),
            Some(EXAMPLE_UTC_EPOCH)
        );
    }

    #[test]
    fn formats_canonical_utc() {
        assert_eq!(
            format_rfc2822(EXAMPLE_UTC_EPOCH),
            Some("Sun, 06 Nov 1994 08:49:37 +0000".to_owned())
        );
    }

    #[test]
    fn formats_derive_the_correct_weekday() {
        // 1970-01-01 was a Thursday (the weekday-index anchor).
        assert_eq!(
            format_rfc2822(0),
            Some("Thu, 01 Jan 1970 00:00:00 +0000".to_owned())
        );
    }

    #[test]
    fn format_parse_round_trips() {
        for epoch in [0, 1, EXAMPLE_UTC_EPOCH, 1_700_000_000, 253_402_300_799] {
            let text = format_rfc2822(epoch).expect("in-range instant formats");
            assert_eq!(parse_rfc2822(&text), Some(epoch), "round-trip for {epoch}");
        }
    }

    #[test]
    fn format_rejects_years_outside_the_four_digit_range() {
        // One second past 9999-12-31T23:59:59Z overflows the fixed-width year.
        assert_eq!(format_rfc2822(253_402_300_800), None);
        assert_eq!(format_rfc2822(-62_167_219_201), None);
    }

    #[test]
    fn rejects_malformed_and_out_of_range_input() {
        assert_eq!(parse_rfc2822("Sun, 06 Foo 1994 08:49:37 GMT"), None);
        assert_eq!(parse_rfc2822("Sun, 32 Nov 1994 08:49:37 GMT"), None);
        assert_eq!(parse_rfc2822("Sun, 06 Nov 1994 24:00:00 GMT"), None);
        assert_eq!(parse_rfc2822("Sun, 06 Nov 1994 08:49:37 +05"), None);
        assert_eq!(parse_rfc2822("Sun, 06 Nov 5 08:49:37 GMT"), None); // one-digit year
        assert_eq!(parse_rfc2822("Sun, 06 Nov 1994 08:49:37 GMT extra"), None);
        assert_eq!(parse_rfc2822(", 06 Nov 1994 08:49:37 GMT"), None); // empty weekday
        assert_eq!(parse_rfc2822(""), None);
        assert_eq!(parse_rfc2822("not a date"), None);
    }

    #[test]
    fn rejects_over_wide_field_widths() {
        // RFC 2822 day is one or two digits; time components are exactly two.
        assert_eq!(parse_rfc2822("Sun, 000006 Nov 1994 08:49:37 GMT"), None);
        assert_eq!(parse_rfc2822("Sun, 06 Nov 1994 008:049:037 GMT"), None);
        assert_eq!(parse_rfc2822("Sun, 06 Nov 1994 08:049 GMT"), None);
        assert_eq!(parse_rfc2822("Sun, 06 Nov 1994 8:49:37 GMT"), None);
    }

    #[test]
    fn rejects_out_of_range_numeric_zone_hours() {
        // RFC 5322 numeric zone hours are 00..=23 and minutes 00..=59.
        assert_eq!(parse_rfc2822("Sun, 06 Nov 1994 08:49:37 +2400"), None);
        assert_eq!(parse_rfc2822("Sun, 06 Nov 1994 08:49:37 -9900"), None);
        assert_eq!(parse_rfc2822("Sun, 06 Nov 1994 08:49:37 +0060"), None);
    }

    #[test]
    fn rejects_non_ascii_zone_without_panicking() {
        // A five-byte non-ASCII zone must return `None`, not panic on a
        // non-char-boundary slice.
        assert_eq!(parse_rfc2822("Sun, 06 Nov 1994 08:49:37 +aéa"), None);
        assert_eq!(parse_rfc2822("Sun, 06 Nov 1994 08:49:37 +xéx"), None);
    }

    #[test]
    fn rejects_the_non_rfc_utc_named_zone() {
        // `UTC` is not an RFC 2822 named zone; `UT`/`GMT` are.
        assert_eq!(parse_rfc2822("Sun, 06 Nov 1994 08:49:37 UTC"), None);
    }

    #[test]
    fn rejects_leap_second_values() {
        // RFC 2822 permits `:60`, but the codec narrows to 0..=59 like RFC 3339.
        assert_eq!(parse_rfc2822("Sat, 31 Dec 2016 23:59:60 +0000"), None);
    }
}
