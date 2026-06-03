//! UTC civil date and epoch conversion helpers.

const SECONDS_PER_DAY: i64 = 86_400;

/// A proleptic Gregorian calendar date.
///
/// Fields are public for lightweight utility use. Conversion functions validate
/// dates before mapping them into epoch-based values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CivilDate {
    /// Calendar year, using astronomical numbering.
    pub year: i64,
    /// Calendar month in the inclusive range `1..=12`.
    pub month: i64,
    /// Calendar day in the inclusive range `1..=31`, constrained by month/year.
    pub day: i64,
}

impl CivilDate {
    /// Returns a validated civil date.
    #[must_use]
    pub fn new(year: i64, month: i64, day: i64) -> Option<Self> {
        let date = Self { year, month, day };
        date.is_valid().then_some(date)
    }

    /// Returns `true` when the date is valid in the proleptic Gregorian calendar.
    #[must_use]
    pub fn is_valid(self) -> bool {
        days_in_month(self.year, self.month)
            .is_some_and(|max_day| self.day >= 1 && self.day <= max_day)
    }
}

/// A UTC civil date and wall-clock time with whole-second precision.
///
/// Fields are public for lightweight utility use. Conversion functions validate
/// date and time ranges before mapping them into epoch-based values.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct CivilDateTime {
    /// UTC calendar date.
    pub date: CivilDate,
    /// Hour of day in the inclusive range `0..=23`.
    pub hour: i64,
    /// Minute of hour in the inclusive range `0..=59`.
    pub minute: i64,
    /// Second of minute in the inclusive range `0..=59`.
    pub second: i64,
}

impl CivilDateTime {
    /// Returns a validated UTC civil date/time.
    #[must_use]
    pub fn new(date: CivilDate, hour: i64, minute: i64, second: i64) -> Option<Self> {
        let datetime = Self {
            date,
            hour,
            minute,
            second,
        };
        datetime.is_valid().then_some(datetime)
    }

    /// Returns `true` when the date and time fields are valid.
    #[must_use]
    pub fn is_valid(self) -> bool {
        self.date.is_valid()
            && (0..24).contains(&self.hour)
            && (0..60).contains(&self.minute)
            && (0..60).contains(&self.second)
    }
}

/// Returns `true` when `year` is a leap year in the proleptic Gregorian calendar.
#[must_use]
pub const fn is_leap_year(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

/// Returns the number of days in `month` for `year`, or `None` for invalid months.
#[must_use]
pub const fn days_in_month(year: i64, month: i64) -> Option<i64> {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => Some(31),
        4 | 6 | 9 | 11 => Some(30),
        2 if is_leap_year(year) => Some(29),
        2 => Some(28),
        _ => None,
    }
}

/// Converts a count of days since the Unix epoch to a civil date.
///
/// Uses Howard Hinnant's `civil_from_days` algorithm.
#[must_use]
pub fn civil_from_days(days_since_unix_epoch: i64) -> Option<CivilDate> {
    civil_from_days_i128(i128::from(days_since_unix_epoch))
}

/// Converts a valid civil date to its day offset from the Unix epoch.
#[must_use]
pub fn days_from_civil(date: CivilDate) -> Option<i64> {
    if !date.is_valid() {
        return None;
    }

    let mut year = i128::from(date.year);
    let month = i128::from(date.month);
    let day = i128::from(date.day);

    if month <= 2 {
        year -= 1;
    }

    let era = year.div_euclid(400);
    let yoe = year - era * 400;
    let mp = month + if month > 2 { -3 } else { 9 };
    let doy = (153 * mp + 2) / 5 + day - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    i64::try_from(era * 146_097 + doe - 719_468).ok()
}

/// Converts Unix epoch seconds to a UTC civil date/time.
#[must_use]
pub const fn datetime_from_epoch_secs(epoch_secs: i64) -> CivilDateTime {
    let days = epoch_secs.div_euclid(SECONDS_PER_DAY);
    let secs_of_day = epoch_secs.rem_euclid(SECONDS_PER_DAY);
    let date = civil_from_epoch_days(days);

    CivilDateTime {
        date,
        hour: secs_of_day / 3_600,
        minute: (secs_of_day % 3_600) / 60,
        second: secs_of_day % 60,
    }
}

/// Converts a valid UTC civil date/time to Unix epoch seconds.
#[must_use]
pub fn epoch_secs_from_datetime(datetime: CivilDateTime) -> Option<i64> {
    if !datetime.is_valid() {
        return None;
    }

    let days = days_from_civil(datetime.date)?;
    let hour_secs = datetime.hour.checked_mul(3_600)?;
    let minute_secs = datetime.minute.checked_mul(60)?;
    let secs_of_day = hour_secs
        .checked_add(minute_secs)?
        .checked_add(datetime.second)?;
    days.checked_mul(SECONDS_PER_DAY)?.checked_add(secs_of_day)
}

const fn civil_from_epoch_days(days_since_unix_epoch: i64) -> CivilDate {
    let z = days_since_unix_epoch + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let mut year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };

    if month <= 2 {
        year += 1;
    }

    CivilDate { year, month, day }
}

fn civil_from_days_i128(days_since_unix_epoch: i128) -> Option<CivilDate> {
    let z = days_since_unix_epoch + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1_460 + doe / 36_524 - doe / 146_096) / 365;
    let mut year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };

    if month <= 2 {
        year += 1;
    }

    Some(CivilDate {
        year: i64::try_from(year).ok()?,
        month: i64::try_from(month).ok()?,
        day: i64::try_from(day).ok()?,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn validates_dates_and_times() {
        assert!(CivilDate::new(2000, 2, 29).is_some());
        assert!(CivilDate::new(1900, 2, 29).is_none());
        assert!(CivilDate::new(2024, 13, 1).is_none());
        assert!(
            CivilDateTime::new(
                CivilDate {
                    year: 2024,
                    month: 1,
                    day: 1
                },
                23,
                59,
                59
            )
            .is_some()
        );
        assert!(
            CivilDateTime::new(
                CivilDate {
                    year: 2024,
                    month: 1,
                    day: 1
                },
                24,
                0,
                0
            )
            .is_none()
        );
        assert!(
            CivilDateTime::new(
                CivilDate {
                    year: 2024,
                    month: 1,
                    day: 1
                },
                -1,
                0,
                0
            )
            .is_none()
        );
        assert!(
            CivilDateTime::new(
                CivilDate {
                    year: 2024,
                    month: 1,
                    day: 1
                },
                0,
                -1,
                0
            )
            .is_none()
        );
        assert!(
            CivilDateTime::new(
                CivilDate {
                    year: 2024,
                    month: 1,
                    day: 1
                },
                0,
                0,
                -1
            )
            .is_none()
        );
    }

    #[test]
    fn converts_days_and_civil_dates() {
        assert_eq!(civil_from_days(0), CivilDate::new(1970, 1, 1));
        assert_eq!(civil_from_days(-1), CivilDate::new(1969, 12, 31));
        assert_eq!(
            days_from_civil(CivilDate {
                year: 1970,
                month: 1,
                day: 1
            }),
            Some(0)
        );
        assert_eq!(
            days_from_civil(CivilDate {
                year: 1969,
                month: 12,
                day: 31
            }),
            Some(-1)
        );
        assert_eq!(
            days_from_civil(CivilDate {
                year: 1900,
                month: 2,
                day: 29
            }),
            None
        );
    }

    #[test]
    fn converts_epoch_seconds_and_datetimes() {
        let cases = [
            (
                -1,
                CivilDateTime {
                    date: CivilDate {
                        year: 1969,
                        month: 12,
                        day: 31,
                    },
                    hour: 23,
                    minute: 59,
                    second: 59,
                },
            ),
            (
                0,
                CivilDateTime {
                    date: CivilDate {
                        year: 1970,
                        month: 1,
                        day: 1,
                    },
                    hour: 0,
                    minute: 0,
                    second: 0,
                },
            ),
            (
                951_782_400,
                CivilDateTime {
                    date: CivilDate {
                        year: 2000,
                        month: 2,
                        day: 29,
                    },
                    hour: 0,
                    minute: 0,
                    second: 0,
                },
            ),
            (
                1_700_000_000,
                CivilDateTime {
                    date: CivilDate {
                        year: 2023,
                        month: 11,
                        day: 14,
                    },
                    hour: 22,
                    minute: 13,
                    second: 20,
                },
            ),
        ];

        for (epoch_secs, datetime) in cases {
            assert_eq!(datetime_from_epoch_secs(epoch_secs), datetime);
            assert_eq!(epoch_secs_from_datetime(datetime), Some(epoch_secs));
        }
    }

    #[test]
    fn rejects_invalid_datetime_before_epoch_conversion() {
        assert_eq!(
            epoch_secs_from_datetime(CivilDateTime {
                date: CivilDate {
                    year: 2024,
                    month: 1,
                    day: 1
                },
                hour: -1,
                minute: 0,
                second: 0,
            }),
            None
        );
    }
}
