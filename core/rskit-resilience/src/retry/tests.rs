//! Tests for the retry policy, backoff strategies, and presets.

use std::time::Duration;

use super::*;

use rskit_errors::{AppError, ErrorCode};
use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};

fn make_policy() -> RetryPolicy {
    RetryPolicy::new()
        .with_max_attempts(3)
        .with_initial_backoff(Duration::from_millis(1))
        .with_jitter(false)
}

#[tokio::test]
async fn execute_succeeds_immediately_on_first_success() {
    let policy = make_policy();
    let result = policy.execute(|| async { Ok::<i32, AppError>(42) }).await;
    assert_eq!(result.unwrap(), 42);
}

#[tokio::test]
async fn execute_retries_and_succeeds_on_second_attempt() {
    let counter = Arc::new(AtomicUsize::new(0));
    let policy = make_policy();

    let result = policy
        .execute(|| {
            let counter = counter.clone();
            async move {
                let attempt = counter.fetch_add(1, Ordering::SeqCst);
                if attempt == 0 {
                    Err(AppError::new(ErrorCode::ConnectionFailed, "test"))
                } else {
                    Ok(99)
                }
            }
        })
        .await;

    assert_eq!(result.unwrap(), 99);
    assert_eq!(counter.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn execute_returns_err_after_exhausting_all_attempts() {
    let counter = Arc::new(AtomicUsize::new(0));
    let policy = make_policy();

    let result = policy
        .execute(|| {
            let counter = counter.clone();
            async move {
                counter.fetch_add(1, Ordering::SeqCst);
                Err::<i32, AppError>(AppError::new(ErrorCode::ConnectionFailed, "test"))
            }
        })
        .await;

    assert!(result.is_err());
    let retry_err = result.unwrap_err();
    assert_eq!(retry_err.attempts, 3);
    assert_eq!(counter.load(Ordering::SeqCst), 3);
}

#[tokio::test]
async fn execute_does_not_retry_non_retryable_error() {
    let counter = Arc::new(AtomicUsize::new(0));
    let policy = make_policy();

    let result = policy
        .execute(|| {
            let counter = counter.clone();
            async move {
                counter.fetch_add(1, Ordering::SeqCst);
                Err::<i32, AppError>(AppError::new(ErrorCode::NotFound, "test"))
            }
        })
        .await;

    assert!(result.is_err());
    assert_eq!(counter.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn execute_with_max_attempts_one_does_not_retry() {
    let counter = Arc::new(AtomicUsize::new(0));
    let policy = RetryPolicy::new()
        .with_max_attempts(1)
        .with_initial_backoff(Duration::from_millis(1))
        .with_jitter(false);

    let result = policy
        .execute(|| {
            let counter = counter.clone();
            async move {
                counter.fetch_add(1, Ordering::SeqCst);
                Err::<i32, AppError>(AppError::new(ErrorCode::ConnectionFailed, "test"))
            }
        })
        .await;

    assert!(result.is_err());
    assert_eq!(counter.load(Ordering::SeqCst), 1);
}

#[test]
fn constant_backoff_uses_same_delay() {
    let policy = RetryPolicy::new()
        .with_constant_backoff(ConstantBackoff::new(Duration::from_millis(25)))
        .with_jitter(false);

    assert_eq!(policy.backoff(1), Duration::from_millis(25));
    assert_eq!(policy.backoff(3), Duration::from_millis(25));
}

#[test]
fn linear_backoff_increases_until_capped() {
    let policy = RetryPolicy::new()
        .with_linear_backoff(LinearBackoff::new(
            Duration::from_millis(10),
            Duration::from_millis(5),
            Duration::from_millis(20),
        ))
        .with_jitter(false);

    assert_eq!(policy.backoff(1), Duration::from_millis(10));
    assert_eq!(policy.backoff(2), Duration::from_millis(15));
    assert_eq!(policy.backoff(3), Duration::from_millis(20));
    assert_eq!(policy.backoff(6), Duration::from_millis(20));
}

#[test]
fn public_backoff_delay_matches_policy_backoff() {
    let policy = RetryPolicy::new()
        .with_initial_backoff(Duration::from_millis(10))
        .with_max_backoff(Duration::from_millis(30))
        .with_jitter(false);

    assert_eq!(policy.backoff_delay(3), Duration::from_millis(30));
}

#[test]
fn retry_presets_create_expected_policies() {
    let fast = RetryPolicy::fast().with_jitter(false);
    assert_eq!(fast.max_attempts, 2);
    assert_eq!(fast.backoff_delay(1), Duration::from_millis(10));

    let standard = RetryPolicy::from_preset(RetryPreset::Standard);
    assert_eq!(standard.max_attempts, 3);
    assert_eq!(standard.max_elapsed_time, Duration::from_secs(10));

    let external = RetryPreset::ExternalService.policy();
    assert_eq!(external.max_attempts, 4);
    assert_eq!(external.max_elapsed_time, Duration::from_secs(30));
}

#[test]
fn seeded_jitter_is_deterministic() {
    let policy = RetryPolicy::new()
        .with_initial_backoff(Duration::from_millis(100))
        .with_jitter_seed(42);

    assert_eq!(policy.backoff_delay(2), policy.backoff_delay(2));
}

#[test]
fn validate_rejects_invalid_retry_limits() {
    assert!(RetryPolicy::new().with_max_attempts(0).validate().is_err());
    assert!(
        RetryPolicy::new()
            .with_backoff_factor(f64::NAN)
            .validate()
            .is_err()
    );
    assert!(
        RetryPolicy::new()
            .with_backoff_factor(0.0)
            .validate()
            .is_err()
    );
}

#[tokio::test]
async fn execute_stops_before_elapsed_time_cap() {
    let policy = RetryPolicy::new()
        .with_max_attempts(10)
        .with_initial_backoff(Duration::from_millis(50))
        .with_jitter(false)
        .with_max_elapsed_time(Duration::from_millis(10));

    let result = policy
        .execute(|| async {
            Err::<(), AppError>(AppError::new(ErrorCode::ConnectionFailed, "test"))
        })
        .await;

    let err = result.unwrap_err();
    assert_eq!(err.attempts, 1);
}

#[tokio::test]
async fn execute_invokes_on_retry_and_honors_retry_if() {
    let retries = Arc::new(AtomicUsize::new(0));
    let seen = Arc::clone(&retries);
    let policy = RetryPolicy::new()
        .with_max_attempts(3)
        .with_initial_backoff(Duration::from_millis(1))
        .with_jitter(false)
        .with_retry_if(|e: &AppError| e.code() == ErrorCode::InvalidInput)
        .with_on_retry(move |_attempt, _err| {
            seen.fetch_add(1, Ordering::SeqCst);
        });

    let attempts = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&attempts);
    let result = policy
        .execute(|| {
            let counter = Arc::clone(&counter);
            async move {
                counter.fetch_add(1, Ordering::SeqCst);
                Err::<(), AppError>(AppError::new(ErrorCode::InvalidInput, "retry me"))
            }
        })
        .await;

    assert!(result.is_err());
    assert_eq!(attempts.load(Ordering::SeqCst), 3);
    assert_eq!(retries.load(Ordering::SeqCst), 2);
}

#[tokio::test]
async fn execute_retry_if_returning_false_stops_a_retryable_error() {
    let policy = RetryPolicy::new()
        .with_max_attempts(5)
        .with_initial_backoff(Duration::from_millis(1))
        .with_jitter(false)
        .with_retry_if(|_e: &AppError| false);

    let attempts = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&attempts);
    let result = policy
        .execute(|| {
            let counter = Arc::clone(&counter);
            async move {
                counter.fetch_add(1, Ordering::SeqCst);
                Err::<(), AppError>(AppError::new(ErrorCode::ConnectionFailed, "conn"))
            }
        })
        .await;

    let err = result.unwrap_err();
    assert_eq!(err.attempts, 1);
    assert_eq!(attempts.load(Ordering::SeqCst), 1);
}

#[tokio::test(start_paused = true)]
async fn execute_times_out_a_single_slow_attempt() {
    let policy = RetryPolicy::new()
        .with_max_attempts(3)
        .with_max_elapsed_time(Duration::from_millis(50))
        .with_jitter(false);

    let result = policy
        .execute(|| async {
            tokio::time::sleep(Duration::from_secs(3600)).await;
            Ok::<(), AppError>(())
        })
        .await;

    let err = result.unwrap_err();
    assert_eq!(err.attempts, 1);
    assert_eq!(err.last_error.code(), ErrorCode::Timeout);
}

#[tokio::test]
async fn execute_returns_error_for_invalid_policy() {
    let policy = RetryPolicy::new().with_max_attempts(0);

    let result = policy.execute(|| async { Ok::<(), AppError>(()) }).await;

    let err = result.unwrap_err();
    assert_eq!(err.attempts, 0);
    assert_eq!(err.last_error.code(), ErrorCode::InvalidInput);
}

#[tokio::test]
async fn execute_times_out_when_elapsed_budget_is_zero() {
    let policy = RetryPolicy::new()
        .with_max_attempts(3)
        .with_max_elapsed_time(Duration::ZERO);

    let attempts = Arc::new(AtomicUsize::new(0));
    let counter = Arc::clone(&attempts);
    let result = policy
        .execute(|| {
            let counter = Arc::clone(&counter);
            async move {
                counter.fetch_add(1, Ordering::SeqCst);
                Ok::<(), AppError>(())
            }
        })
        .await;

    let err = result.unwrap_err();
    assert_eq!(err.attempts, 0);
    assert_eq!(err.last_error.code(), ErrorCode::Timeout);
    assert_eq!(attempts.load(Ordering::SeqCst), 0);
}
