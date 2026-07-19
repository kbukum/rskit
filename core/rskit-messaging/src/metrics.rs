//! Operational metrics collection for messaging operations.

use std::time::Duration;

/// Collects messaging operational metrics.
///
/// Implementations record timing and success/failure information for publish and consume operations,
/// enabling observability without coupling broker logic to a specific metrics adapter.
pub trait MetricsCollector: Send + Sync + 'static {
    /// Records a publish operation outcome.
    fn record_publish(&self, topic: &str, duration: Duration, success: bool);

    /// Records a consume operation outcome.
    fn record_consume(&self, topic: &str, duration: Duration, success: bool);
}

/// No-op metrics collector for when metrics are disabled.
///
/// Every method is a no-op, making this suitable as a default when no metrics adapter is configured.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoopMetrics;

impl MetricsCollector for NoopMetrics {
    fn record_publish(&self, _topic: &str, _duration: Duration, _success: bool) {}

    fn record_consume(&self, _topic: &str, _duration: Duration, _success: bool) {}
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn noop_metrics_does_not_panic() {
        let m = NoopMetrics;
        m.record_publish("t", Duration::from_millis(10), true);
        m.record_consume("t", Duration::from_millis(5), false);
    }
}
