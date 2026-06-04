use rskit_errors::ErrorCode;
use rskit_resilience::{Bulkhead, BulkheadConfig, CbConfig, CbState, CircuitBreaker, RetryPolicy};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};
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
                    Err(rskit_errors::AppError::new(
                        ErrorCode::ServiceUnavailable,
                        "fail",
                    ))
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
    let cb = CircuitBreaker::new(CbConfig::new("test").with_max_failures(2)).unwrap();
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
    let bh = Bulkhead::new(BulkheadConfig::new("test-bh", 1)).unwrap();
    let bh2 = bh.clone();

    // Hold one permit via a long-running execute
    let barrier = Arc::new(tokio::sync::Notify::new());
    let barrier2 = barrier.clone();
    let handle = tokio::spawn(async move {
        bh2.execute(|| async move {
            barrier2.notified().await;
            Ok(())
        })
        .await
    });

    // Give the first task a moment to acquire its permit
    tokio::time::sleep(Duration::from_millis(50)).await;

    // Second execute should be rejected because the only slot is occupied
    let result = bh
        .execute(|| async { Ok::<_, rskit_errors::AppError>(()) })
        .await;
    assert!(result.is_err());

    // Release the first task
    barrier.notify_one();
    let _ = handle.await;
}
