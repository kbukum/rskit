//! Named retry presets for common infrastructure integration patterns.

use std::time::Duration;

use super::backoff::ConstantBackoff;
use super::policy::RetryPolicy;

/// Named retry configurations for common infrastructure integration patterns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum RetryPreset {
    /// Short retry loop for local tests and latency-sensitive operations.
    Fast,
    /// Balanced default for general service-to-service calls.
    Standard,
    /// More tolerant policy for external network dependencies.
    ExternalService,
}
impl RetryPreset {
    /// Build the retry policy represented by this preset.
    #[must_use]
    pub fn policy(self) -> RetryPolicy {
        match self {
            Self::Fast => RetryPolicy::new()
                .with_max_attempts(2)
                .with_constant_backoff(ConstantBackoff::new(Duration::from_millis(10)))
                .with_max_elapsed_time(Duration::from_secs(1)),
            Self::Standard => RetryPolicy::new()
                .with_max_attempts(3)
                .with_initial_backoff(Duration::from_millis(100))
                .with_max_backoff(Duration::from_secs(2))
                .with_max_elapsed_time(Duration::from_secs(10)),
            Self::ExternalService => RetryPolicy::new()
                .with_max_attempts(4)
                .with_initial_backoff(Duration::from_millis(200))
                .with_max_backoff(Duration::from_secs(5))
                .with_max_elapsed_time(Duration::from_secs(30)),
        }
    }
}
