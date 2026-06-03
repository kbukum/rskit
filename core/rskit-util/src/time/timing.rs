//! Execution timing helpers.

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_time_it() {
        let (res, elapsed) = time_it(|| 42);
        assert_eq!(res, 42);
        assert!(elapsed <= Duration::from_secs(1));
    }
}
