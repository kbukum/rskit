use std::time::Duration;

use rskit_errors::AppError;
use rskit_resilience::{
    BulkheadConfig, CbConfig, ConstantBackoff, Policy, RateLimiterConfig, RetryPolicy,
};

#[tokio::test]
async fn compose_rate_bulkhead_circuit_timeout_retry() {
    let policy = Policy::new()
        .try_with_rate_limiter_config(RateLimiterConfig::new("example-rate", 10, 1))
        .unwrap()
        .with_bulkhead(BulkheadConfig::new("example-bulkhead", 2))
        .with_circuit_breaker(CbConfig::new("example-circuit").with_max_failures(2))
        .with_timeout(Duration::from_secs(1))
        .with_retry(
            RetryPolicy::new()
                .with_max_attempts(3)
                .with_constant_backoff(ConstantBackoff::new(Duration::from_millis(1)))
                .with_jitter(false),
        );

    let mut attempts = 0usize;
    let result = policy
        .execute(|| {
            attempts += 1;
            let attempt = attempts;
            async move {
                if attempt == 1 {
                    Err::<u32, AppError>(AppError::connection_failed("transient upstream error"))
                } else {
                    Ok(42)
                }
            }
        })
        .await;

    assert_eq!(result.unwrap(), 42);
    assert_eq!(attempts, 2);
}
