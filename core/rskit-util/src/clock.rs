//! Deterministic clock abstraction for testable time-dependent code.
//!
//! The [`Clock`] trait lets production code call [`Clock::now`] while tests
//! inject a [`FakeClock`] with a controllable instant.

use chrono::{DateTime, TimeDelta, Utc};
use parking_lot::Mutex;

/// Trait for clock implementations — enables deterministic testing.
pub trait Clock: Send + Sync {
    /// Return the current UTC time.
    fn now(&self) -> DateTime<Utc>;
}

/// Real clock backed by `Utc::now()`.
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> DateTime<Utc> {
        Utc::now()
    }
}

/// Deterministic clock for tests.  Starts at a fixed time; advance manually.
pub struct FakeClock {
    now: Mutex<DateTime<Utc>>,
}

impl FakeClock {
    /// Create a new `FakeClock` starting at `initial`, or 2024-01-01T00:00:00Z
    /// if `None`.
    pub fn new(initial: Option<DateTime<Utc>>) -> Self {
        let default = DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z")
            .expect("valid constant")
            .to_utc();
        Self {
            now: Mutex::new(initial.unwrap_or(default)),
        }
    }

    /// Advance time by the given duration.
    pub fn advance(&self, delta: TimeDelta) {
        let mut now = self.now.lock();
        *now += delta;
    }

    /// Set absolute time.
    pub fn set(&self, dt: DateTime<Utc>) {
        let mut now = self.now.lock();
        *now = dt;
    }
}

impl Clock for FakeClock {
    fn now(&self) -> DateTime<Utc> {
        *self.now.lock()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_clock_returns_utc() {
        let clock = SystemClock;
        let now = clock.now();
        let diff = Utc::now() - now;
        assert!(diff.num_seconds().abs() < 2);
    }

    #[test]
    fn fake_clock_default_start() {
        let clock = FakeClock::new(None);
        let expected = DateTime::parse_from_rfc3339("2024-01-01T00:00:00Z")
            .unwrap()
            .to_utc();
        assert_eq!(clock.now(), expected);
    }

    #[test]
    fn fake_clock_advance() {
        let clock = FakeClock::new(None);
        clock.advance(TimeDelta::seconds(30));
        let expected = DateTime::parse_from_rfc3339("2024-01-01T00:00:30Z")
            .unwrap()
            .to_utc();
        assert_eq!(clock.now(), expected);
    }

    #[test]
    fn fake_clock_set() {
        let clock = FakeClock::new(None);
        let new_time = DateTime::parse_from_rfc3339("2025-06-15T12:00:00Z")
            .unwrap()
            .to_utc();
        clock.set(new_time);
        assert_eq!(clock.now(), new_time);
    }

    #[test]
    fn fake_clock_custom_initial() {
        let initial = DateTime::parse_from_rfc3339("2020-05-01T08:30:00Z")
            .unwrap()
            .to_utc();
        let clock = FakeClock::new(Some(initial));
        assert_eq!(clock.now(), initial);
    }
}
