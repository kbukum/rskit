use rskit_resilience::{RetryPolicy, CircuitBreaker, CbConfig, CbState, Bulkhead, BulkheadConfig};
use rskit_errors::ErrorCode;
use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::Arc;
use std::time::Duration;

#[tokio::test]
async fn retry_policy_retries_on_failure() {
    let attempts = Arc::new(AtomicU32::new(0));
    let attempts2 = attempts.clone();

    let policy = RetryPolicy::new().with_max_attempts(3);
    let result = policy
        .execute(|| {
            let c = attempts2.clone();
            async move {
                let n = c.fetch_add(1, Ordering::SeqCst);
                if n < 2 {
                    Err(rskit_errors::AppError::new(ErrorCode::Internal, "fail"))
                } else {
                    Ok("ok")
                }
            }
        })
        .await;

    assert!(result.is_ok());
    assert_eq!(attempts.load(Ordering::SeqCst), 3);
}

#[tokio::test]
async fn circuit_breaker_opens_after_failures() {
    let cb = CircuitBreaker::new(CbConfig::new("test").with_max_failures(2));
    assert_eq!(cb.state(), CbState::Closed);

    for _ in 0..2 {
        let _ = cb
            .execute(|| async {
                Err::<(), _>(rskit_errors::AppError::new(ErrorCode::Internal, "fail"))
            })
            .await;
    }

    assert_eq!(cb.state(), CbState::Open);
}

#[tokio::test]
async fn bulkhead_rejects_over_limit() {
    let bh = Bulkhead::new(BulkheadConfig::new(1, Duration::from_millis(10)));
    // Acquire the only permit
    let _permit = bh.acquire().await.unwrap();
    // Second acquire should time out and return an error
    let result = bh.acquire().await;
    assert!(result.is_err());
}
