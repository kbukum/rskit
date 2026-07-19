//! Clock abstractions for deterministic time-dependent code.

use std::sync::Arc;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

/// Shared clock handle.
pub type SharedClock = Arc<dyn Clock>;

/// Clock used for wall-clock timestamps and monotonic elapsed durations.
///
/// Production code normally uses [`SystemClock`]. Tests
/// and reproducible harnesses can inject [`FixedClock`] or a domain-specific implementation.
pub trait Clock: Send + Sync {
    /// Monotonic milliseconds used for elapsed-duration measurements.
    fn monotonic_millis(&self) -> u64;

    /// Unix epoch seconds used for wall-clock timestamps.
    fn epoch_seconds(&self) -> u64;
}

/// System-backed clock.
#[derive(Debug)]
pub struct SystemClock {
    started_at: Instant,
}

impl Default for SystemClock {
    fn default() -> Self {
        Self::new()
    }
}

impl SystemClock {
    /// Create a new system-backed clock.
    #[must_use]
    pub fn new() -> Self {
        Self {
            started_at: Instant::now(),
        }
    }
}

impl Clock for SystemClock {
    fn monotonic_millis(&self) -> u64 {
        u64::try_from(self.started_at.elapsed().as_millis()).unwrap_or(u64::MAX)
    }

    fn epoch_seconds(&self) -> u64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .ok()
            .map_or(0, |duration| duration.as_secs())
    }
}

/// Deterministic clock for tests and reproducible harnesses.
#[derive(Debug, Clone)]
pub struct FixedClock {
    epoch_seconds: u64,
    monotonic_millis: u64,
}

impl FixedClock {
    /// Create a fixed clock snapshot.
    #[must_use]
    pub const fn new(epoch_seconds: u64, monotonic_millis: u64) -> Self {
        Self {
            epoch_seconds,
            monotonic_millis,
        }
    }
}

impl Clock for FixedClock {
    fn monotonic_millis(&self) -> u64 {
        self.monotonic_millis
    }

    fn epoch_seconds(&self) -> u64 {
        self.epoch_seconds
    }
}

/// Return a shared system-backed clock.
#[must_use]
pub fn system_clock() -> SharedClock {
    Arc::new(SystemClock::new())
}

#[cfg(test)]
mod tests {
    use super::{Clock, FixedClock};

    #[test]
    fn fixed_clock_returns_injected_values() {
        let clock = FixedClock::new(1_700_000_000, 42);

        assert_eq!(clock.epoch_seconds(), 1_700_000_000);
        assert_eq!(clock.monotonic_millis(), 42);
    }
}
