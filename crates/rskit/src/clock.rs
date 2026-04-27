//! Mockable clock abstraction for deterministic time-dependent testing.

use std::time::{Duration, SystemTime};

#[cfg(any(test, feature = "test-util"))]
#[allow(clippy::disallowed_types)]
use std::sync::{Arc, Mutex};

/// Abstraction over wall-clock time for testability.
pub trait Clock: Send + Sync + 'static {
    /// Current wall-clock time.
    fn now(&self) -> SystemTime;

    /// Monotonic instant (for timeouts / rate-limits).
    fn elapsed_since(&self, earlier: SystemTime) -> Duration {
        self.now().duration_since(earlier).unwrap_or(Duration::ZERO)
    }
}

/// Production clock — delegates to `SystemTime::now()`.
#[derive(Debug, Clone, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now(&self) -> SystemTime {
        SystemTime::now()
    }
}

/// Manually-controllable clock for unit tests.
///
/// # Example
/// ```
/// use rskit::clock::{Clock, MockClock};
/// use std::time::Duration;
///
/// let clock = MockClock::default();
/// let t0 = clock.now();
/// clock.advance(Duration::from_secs(5));
/// assert!(clock.now() > t0);
/// ```
#[cfg(any(test, feature = "test-util"))]
#[allow(clippy::disallowed_types)] // MockClock is test-only, never crosses async boundaries
#[derive(Debug, Clone)]
pub struct MockClock {
    current: Arc<Mutex<SystemTime>>,
}

#[cfg(any(test, feature = "test-util"))]
#[allow(clippy::disallowed_types)]
impl Default for MockClock {
    fn default() -> Self {
        Self {
            current: Arc::new(Mutex::new(SystemTime::now())),
        }
    }
}

#[cfg(any(test, feature = "test-util"))]
impl MockClock {
    /// Advance the mock clock by `duration`.
    pub fn advance(&self, duration: Duration) {
        let mut t = self.current.lock().expect("clock poisoned");
        *t += duration;
    }

    /// Set the mock clock to a specific time.
    pub fn set(&self, time: SystemTime) {
        *self.current.lock().expect("clock poisoned") = time;
    }
}

#[cfg(any(test, feature = "test-util"))]
impl Clock for MockClock {
    fn now(&self) -> SystemTime {
        *self.current.lock().expect("clock poisoned")
    }
}
