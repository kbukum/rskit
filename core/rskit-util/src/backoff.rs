//! Stateless mathematical backoff calculations.

use std::time::Duration;

/// Calculate a capped exponential backoff delay.
///
/// Completely stateless and does not invoke thread sleep or tokio timers.
///
/// # Examples
///
/// ```
/// use std::time::Duration;
/// use rskit_util::backoff::calculate_backoff;
///
/// let min = Duration::from_millis(100);
/// let max = Duration::from_secs(10);
///
/// // First attempt:
/// let backoff_1 = calculate_backoff(1, min, max);
/// assert!(backoff_1 >= min);
/// ```
pub fn calculate_backoff(attempt: u32, min_delay: Duration, max_delay: Duration) -> Duration {
    if max_delay <= min_delay {
        return min_delay;
    }
    if attempt <= 1 {
        return min_delay;
    }
    if min_delay.is_zero() {
        return min_delay;
    }

    let mut delay = min_delay;
    for _ in 1..attempt {
        let Some(next) = delay.checked_mul(2) else {
            return max_delay;
        };
        if next >= max_delay {
            return max_delay;
        }
        delay = next;
    }
    delay
}

/// Calculate a deterministic jittered delay inside `[min_delay, capped_backoff]`.
///
/// `jitter_permyriad` is an integer ratio in the inclusive range `0..=10_000`,
/// where `0` returns `min_delay` and `10_000` returns the capped exponential delay.
#[must_use]
pub fn calculate_jittered_backoff(
    attempt: u32,
    min_delay: Duration,
    max_delay: Duration,
    jitter_permyriad: u16,
) -> Option<Duration> {
    if jitter_permyriad > 10_000 {
        return None;
    }
    let limit = calculate_backoff(attempt, min_delay, max_delay);
    if limit <= min_delay {
        return Some(min_delay);
    }

    let min_nanos = min_delay.as_nanos();
    let span_nanos = limit.as_nanos().saturating_sub(min_nanos);
    let jitter_nanos = span_nanos
        .checked_mul(u128::from(jitter_permyriad))?
        .checked_div(10_000)?;
    min_delay.checked_add(duration_from_nanos(jitter_nanos)?)
}

fn duration_from_nanos(nanos: u128) -> Option<Duration> {
    let secs = nanos / 1_000_000_000;
    let subsec_nanos = nanos % 1_000_000_000;
    Some(Duration::new(
        u64::try_from(secs).ok()?,
        u32::try_from(subsec_nanos).ok()?,
    ))
}

/// Reusable exponential backoff calculator configuration.
#[derive(Debug, Clone, Copy)]
pub struct ExponentialBackoff {
    min_delay: Duration,
    max_delay: Duration,
}

impl ExponentialBackoff {
    /// Create a new backoff calculator config.
    #[must_use]
    pub const fn new(min_delay: Duration, max_delay: Duration) -> Self {
        Self {
            min_delay,
            max_delay,
        }
    }

    /// Calculate the delay for a given attempt index (starting at 1).
    #[must_use]
    pub fn delay(&self, attempt: u32) -> Duration {
        calculate_backoff(attempt, self.min_delay, self.max_delay)
    }

    /// Calculate a jittered delay for a given attempt using an injected factor.
    #[must_use]
    pub fn jittered_delay(&self, attempt: u32, jitter_permyriad: u16) -> Option<Duration> {
        calculate_jittered_backoff(attempt, self.min_delay, self.max_delay, jitter_permyriad)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_calculate_backoff_bounds() {
        let min = Duration::from_millis(100);
        let max = Duration::from_secs(5);

        for attempt in 1..10 {
            let backoff = calculate_backoff(attempt, min, max);
            assert!(backoff >= min, "delay {backoff:?} < min {min:?}");
            assert!(backoff <= max, "delay {backoff:?} > max {max:?}");
        }
    }

    #[test]
    fn test_calculate_backoff_reaches_cap_for_high_attempts() {
        let min = Duration::from_nanos(1);
        let max = Duration::from_secs(10);

        assert_eq!(calculate_backoff(64, min, max), max);
    }

    #[test]
    fn test_calculate_jittered_backoff_is_deterministic() {
        let min = Duration::from_millis(100);
        let max = Duration::from_secs(5);

        assert_eq!(calculate_jittered_backoff(3, min, max, 0), Some(min));
        assert_eq!(
            calculate_jittered_backoff(3, min, max, 10_000),
            Some(calculate_backoff(3, min, max))
        );
        assert_eq!(calculate_jittered_backoff(3, min, max, 10_001), None);
    }

    #[test]
    fn test_exponential_backoff_struct() {
        let backoff = ExponentialBackoff::new(Duration::from_millis(50), Duration::from_secs(1));
        let delay_1 = backoff.delay(1);
        assert!(delay_1 >= Duration::from_millis(50));
        assert!(delay_1 <= Duration::from_secs(1));
        assert!(backoff.jittered_delay(1, 5_000).is_some());
    }
}
