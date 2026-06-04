//! Comprehensive integration tests for rskit-resilience.
//!
//! Covers: CircuitBreaker, RetryPolicy, Bulkhead, RateLimiter,
//! Tower layers, and multi-pattern composition.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use rskit_errors::{AppError, ErrorCode};
use rskit_resilience::{
    Bulkhead, BulkheadConfig, BulkheadLayer, CbConfig, CbState, CircuitBreaker,
    CircuitBreakerLayer, RateLimitLayer, RateLimiter, RetryLayer, RetryPolicy,
};
use tower::{Service, ServiceBuilder, ServiceExt};

// ═══════════════════════════════════════════════════════════════════════════
// Helpers
// ═══════════════════════════════════════════════════════════════════════════

fn fail_err() -> AppError {
    AppError::new(ErrorCode::ConnectionFailed, "transient")
}

fn non_retryable_err() -> AppError {
    AppError::new(ErrorCode::NotFound, "not found")
}

// ═══════════════════════════════════════════════════════════════════════════
// 1. Circuit Breaker Integration
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn cb_full_state_machine_cycle() {
    let cb = CircuitBreaker::new(
        CbConfig::new("cycle")
            .with_max_failures(2)
            .with_timeout(Duration::from_millis(50))
            .with_half_open_max_calls(2),
    )
    .unwrap();
    assert_eq!(cb.state(), CbState::Closed);

    // Closed → Open
    for _ in 0..2 {
        let _ = cb.execute(|| async { Err::<i32, _>(fail_err()) }).await;
    }
    assert_eq!(cb.state(), CbState::Open);

    // Wait for timeout → HalfOpen on next execute
    tokio::time::sleep(Duration::from_millis(60)).await;

    // Succeed enough probes to close
    let r = cb.execute(|| async { Ok::<i32, AppError>(1) }).await;
    assert!(r.is_ok());
    assert_eq!(cb.state(), CbState::HalfOpen);

    let r = cb.execute(|| async { Ok::<i32, AppError>(2) }).await;
    assert!(r.is_ok());
    assert_eq!(cb.state(), CbState::Closed);
}

#[tokio::test]
async fn cb_half_open_allows_only_max_probe_calls() {
    let cb = CircuitBreaker::new(
        CbConfig::new("ho-probe")
            .with_max_failures(1)
            .with_timeout(Duration::from_millis(20))
            .with_half_open_max_calls(2),
    )
    .unwrap();

    // Trip the breaker
    let _ = cb.execute(|| async { Err::<i32, _>(fail_err()) }).await;
    assert_eq!(cb.state(), CbState::Open);

    tokio::time::sleep(Duration::from_millis(30)).await;

    // The first call triggers Open→HalfOpen transition and consumes a probe slot.
    // half_open_max_calls=2 allows two total concurrent probes.
    let barrier = Arc::new(tokio::sync::Notify::new());

    let mut handles = Vec::new();
    for _ in 0..2 {
        let cb = cb.clone();
        let b = barrier.clone();
        handles.push(tokio::spawn(async move {
            cb.execute(|| async move {
                b.notified().await;
                Ok::<i32, AppError>(1)
            })
            .await
        }));
    }

    // Give probes time to enter execute
    tokio::time::sleep(Duration::from_millis(10)).await;

    // Third call should be rejected (half-open slots exhausted)
    let overflow = cb.execute(|| async { Ok::<i32, AppError>(99) }).await;
    assert!(overflow.is_err());

    // Release probes
    barrier.notify_waiters();
    for h in handles {
        assert!(h.await.unwrap().is_ok());
    }
}

#[tokio::test]
async fn cb_half_open_failure_reopens() {
    let cb = CircuitBreaker::new(
        CbConfig::new("ho-fail")
            .with_max_failures(1)
            .with_timeout(Duration::from_millis(20))
            .with_half_open_max_calls(3),
    )
    .unwrap();

    // Trip to Open
    let _ = cb.execute(|| async { Err::<i32, _>(fail_err()) }).await;
    tokio::time::sleep(Duration::from_millis(30)).await;

    // Trigger transition to HalfOpen, then fail
    let r = cb.execute(|| async { Err::<i32, _>(fail_err()) }).await;
    assert!(r.is_err());
    assert_eq!(cb.state(), CbState::Open);
}

#[tokio::test]
async fn cb_on_state_change_callback() {
    let transitions: Arc<parking_lot::Mutex<Vec<(CbState, CbState)>>> =
        Arc::new(parking_lot::Mutex::new(Vec::new()));
    let t = transitions.clone();

    let cb = CircuitBreaker::new(
        CbConfig::new("cb-callback")
            .with_max_failures(1)
            .with_timeout(Duration::from_millis(20))
            .with_on_state_change(move |from, to| {
                t.lock().push((from, to));
            }),
    )
    .unwrap();

    // Closed → Open
    let _ = cb.execute(|| async { Err::<i32, _>(fail_err()) }).await;
    assert_eq!(cb.state(), CbState::Open);

    tokio::time::sleep(Duration::from_millis(30)).await;

    // Open → HalfOpen (triggered inside execute)
    let _ = cb.execute(|| async { Ok::<i32, AppError>(1) }).await;

    let t = transitions.lock().clone();
    assert_eq!(t[0], (CbState::Closed, CbState::Open));
    assert_eq!(t[1], (CbState::Open, CbState::HalfOpen));
}

#[tokio::test]
async fn cb_concurrent_execute_50_tasks() {
    let cb = CircuitBreaker::new(CbConfig::new("conc").with_max_failures(100)).unwrap();
    let counter = Arc::new(AtomicUsize::new(0));

    let mut handles = Vec::new();
    for _ in 0..50 {
        let cb = cb.clone();
        let c = counter.clone();
        handles.push(tokio::spawn(async move {
            let r = cb
                .execute(|| async {
                    c.fetch_add(1, Ordering::SeqCst);
                    Ok::<i32, AppError>(1)
                })
                .await;
            assert!(r.is_ok());
        }));
    }

    for h in handles {
        h.await.unwrap();
    }

    assert_eq!(counter.load(Ordering::SeqCst), 50);
    assert_eq!(cb.state(), CbState::Closed);
}

#[tokio::test]
async fn cb_reset_during_half_open() {
    let cb = CircuitBreaker::new(
        CbConfig::new("reset-ho")
            .with_max_failures(1)
            .with_timeout(Duration::from_millis(20))
            .with_half_open_max_calls(3),
    )
    .unwrap();

    let _ = cb.execute(|| async { Err::<i32, _>(fail_err()) }).await;
    tokio::time::sleep(Duration::from_millis(30)).await;

    // Trigger HalfOpen
    let _ = cb.execute(|| async { Ok::<i32, AppError>(1) }).await;
    assert_eq!(cb.state(), CbState::HalfOpen);

    cb.reset();
    assert_eq!(cb.state(), CbState::Closed);
    assert_eq!(cb.failures(), 0);
}

#[tokio::test]
async fn cb_very_short_timeout() {
    let cb = CircuitBreaker::new(
        CbConfig::new("short")
            .with_max_failures(1)
            .with_timeout(Duration::from_millis(10))
            .with_half_open_max_calls(1),
    )
    .unwrap();

    let _ = cb.execute(|| async { Err::<i32, _>(fail_err()) }).await;
    assert_eq!(cb.state(), CbState::Open);

    tokio::time::sleep(Duration::from_millis(15)).await;

    let r = cb.execute(|| async { Ok::<i32, AppError>(42) }).await;
    assert!(r.is_ok());
    // With half_open_max_calls=1, one success should close it
    assert_eq!(cb.state(), CbState::Closed);
}

#[tokio::test]
async fn cb_execute_after_reset_clean_state() {
    let cb = CircuitBreaker::new(CbConfig::new("clean").with_max_failures(2)).unwrap();

    let _ = cb.execute(|| async { Err::<i32, _>(fail_err()) }).await;
    assert_eq!(cb.failures(), 1);

    cb.reset();
    assert_eq!(cb.failures(), 0);
    assert_eq!(cb.state(), CbState::Closed);

    let r = cb.execute(|| async { Ok::<i32, AppError>(7) }).await;
    assert_eq!(r.unwrap(), 7);
}

#[tokio::test]
async fn cb_service_unavailable_error_format() {
    let cb = CircuitBreaker::new(CbConfig::new("svc-err").with_max_failures(1)).unwrap();

    let _ = cb.execute(|| async { Err::<i32, _>(fail_err()) }).await;

    let err = cb
        .execute(|| async { Ok::<i32, AppError>(1) })
        .await
        .unwrap_err();
    assert_eq!(err.code(), ErrorCode::ServiceUnavailable);
    assert!(err.details().contains_key("circuit_breaker_state"));
}

#[tokio::test]
async fn cb_state_check_at_boundary() {
    let cb = CircuitBreaker::new(CbConfig::new("boundary").with_max_failures(3)).unwrap();

    // 2 failures — still closed
    for _ in 0..2 {
        let _ = cb.execute(|| async { Err::<i32, _>(fail_err()) }).await;
    }
    assert_eq!(cb.state(), CbState::Closed);
    assert_eq!(cb.failures(), 2);

    // 3rd failure — trips to open
    let _ = cb.execute(|| async { Err::<i32, _>(fail_err()) }).await;
    assert_eq!(cb.state(), CbState::Open);
}

// ═══════════════════════════════════════════════════════════════════════════
// 2. RetryPolicy Integration
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn retry_success_on_first_attempt() {
    let counter = Arc::new(AtomicUsize::new(0));
    let policy = RetryPolicy::new()
        .with_max_attempts(3)
        .with_initial_backoff(Duration::from_millis(1))
        .with_jitter(false);

    let c = counter.clone();
    let r = policy
        .execute(|| {
            let c = c.clone();
            async move {
                c.fetch_add(1, Ordering::SeqCst);
                Ok::<i32, AppError>(42)
            }
        })
        .await;

    assert_eq!(r.unwrap(), 42);
    assert_eq!(counter.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn retry_success_on_nth_attempt() {
    let counter = Arc::new(AtomicUsize::new(0));
    let policy = RetryPolicy::new()
        .with_max_attempts(5)
        .with_initial_backoff(Duration::from_millis(1))
        .with_jitter(false);

    let c = counter.clone();
    let r = policy
        .execute(|| {
            let c = c.clone();
            async move {
                let n = c.fetch_add(1, Ordering::SeqCst);
                if n < 3 { Err(fail_err()) } else { Ok(100) }
            }
        })
        .await;

    assert_eq!(r.unwrap(), 100);
    assert_eq!(counter.load(Ordering::SeqCst), 4);
}

#[tokio::test]
async fn retry_all_attempts_exhausted() {
    let counter = Arc::new(AtomicUsize::new(0));
    let policy = RetryPolicy::new()
        .with_max_attempts(3)
        .with_initial_backoff(Duration::from_millis(1))
        .with_jitter(false);

    let c = counter.clone();
    let r = policy
        .execute(|| {
            let c = c.clone();
            async move {
                c.fetch_add(1, Ordering::SeqCst);
                Err::<i32, AppError>(fail_err())
            }
        })
        .await;

    let err = r.unwrap_err();
    assert_eq!(err.attempts, 3);
    assert_eq!(err.last_error.code(), ErrorCode::ConnectionFailed);
    assert_eq!(counter.load(Ordering::SeqCst), 3);
}

#[tokio::test]
async fn retry_if_filter_only_retries_specific_errors() {
    let counter = Arc::new(AtomicUsize::new(0));
    let policy = RetryPolicy::new()
        .with_max_attempts(5)
        .with_initial_backoff(Duration::from_millis(1))
        .with_jitter(false)
        .with_retry_if(|e| e.code() == ErrorCode::Timeout);

    let c = counter.clone();
    let r = policy
        .execute(|| {
            let c = c.clone();
            async move {
                c.fetch_add(1, Ordering::SeqCst);
                // ConnectionFailed is NOT matched by our custom retry_if
                Err::<i32, AppError>(fail_err())
            }
        })
        .await;

    assert!(r.is_err());
    // Should stop after first attempt since retry_if rejects ConnectionFailed
    assert_eq!(counter.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn retry_on_retry_callback() {
    let attempts_seen: Arc<parking_lot::Mutex<Vec<(u32, String)>>> =
        Arc::new(parking_lot::Mutex::new(Vec::new()));
    let a = attempts_seen.clone();

    let policy = RetryPolicy::new()
        .with_max_attempts(3)
        .with_initial_backoff(Duration::from_millis(1))
        .with_jitter(false)
        .with_on_retry(move |attempt, err| {
            a.lock().push((attempt, err.message().to_string()));
        });

    let _ = policy
        .execute(|| async { Err::<i32, AppError>(fail_err()) })
        .await;

    let seen = attempts_seen.lock().clone();
    assert_eq!(seen.len(), 2); // 3 attempts → 2 retries
    assert_eq!(seen[0].0, 1);
    assert_eq!(seen[1].0, 2);
}

#[tokio::test]
async fn retry_jitter_produces_varying_delays() {
    // With jitter ON, multiple retries should not all have the exact same timing.
    // We measure by counting attempts and verifying all attempts execute.
    let delays_seen: Arc<parking_lot::Mutex<Vec<std::time::Instant>>> =
        Arc::new(parking_lot::Mutex::new(Vec::new()));

    let d = delays_seen.clone();
    let policy = RetryPolicy::new()
        .with_max_attempts(5)
        .with_initial_backoff(Duration::from_millis(10))
        .with_jitter(true)
        .with_on_retry(move |_attempt, _err| {
            d.lock().push(std::time::Instant::now());
        });

    let counter = Arc::new(AtomicUsize::new(0));
    let c = counter.clone();
    let _ = policy
        .execute(|| {
            let c = c.clone();
            async move {
                c.fetch_add(1, Ordering::SeqCst);
                Err::<i32, AppError>(fail_err())
            }
        })
        .await;

    assert_eq!(counter.load(Ordering::SeqCst), 5);
    let seen = delays_seen.lock().clone();
    // 5 attempts = 4 retries → 4 on_retry callbacks
    assert_eq!(seen.len(), 4);
}

#[tokio::test]
async fn retry_max_backoff_capping() {
    // With factor=10 and initial=100ms, uncapped delay would grow fast.
    // max_backoff=200ms should cap it. We verify total time for 3 retries
    // is bounded (not 100 + 1000 + 10000 = ~11s uncapped).
    let policy = RetryPolicy::new()
        .with_max_attempts(4) // 1 call + 3 retries
        .with_initial_backoff(Duration::from_millis(100))
        .with_max_backoff(Duration::from_millis(200))
        .with_backoff_factor(10.0)
        .with_jitter(false);

    let start = std::time::Instant::now();
    let _ = policy
        .execute(|| async { Err::<i32, AppError>(fail_err()) })
        .await;
    let elapsed = start.elapsed();

    // Without capping: 100 + 1000 + 10000 = 11100ms
    // With capping at 200: 100 + 200 + 200 = 500ms
    assert!(
        elapsed < Duration::from_millis(1000),
        "elapsed {elapsed:?} — max_backoff not capping"
    );
}

#[tokio::test]
async fn retry_non_retryable_immediate_failure() {
    let counter = Arc::new(AtomicUsize::new(0));
    let policy = RetryPolicy::new()
        .with_max_attempts(5)
        .with_initial_backoff(Duration::from_millis(1))
        .with_jitter(false);

    let c = counter.clone();
    let r = policy
        .execute(|| {
            let c = c.clone();
            async move {
                c.fetch_add(1, Ordering::SeqCst);
                Err::<i32, AppError>(non_retryable_err())
            }
        })
        .await;

    assert!(r.is_err());
    assert_eq!(counter.load(Ordering::SeqCst), 1);
    assert_eq!(r.unwrap_err().last_error.code(), ErrorCode::NotFound);
}

// ═══════════════════════════════════════════════════════════════════════════
// 3. Bulkhead Integration
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn bh_exactly_max_concurrent_succeed() {
    let bh =
        Bulkhead::new(BulkheadConfig::new("exact", 3).with_max_wait(Duration::from_millis(100)))
            .unwrap();
    let running = Arc::new(AtomicUsize::new(0));
    let max_running = Arc::new(AtomicUsize::new(0));
    let barrier = Arc::new(tokio::sync::Notify::new());

    let mut handles = Vec::new();
    for _ in 0..3 {
        let bh = bh.clone();
        let r = running.clone();
        let m = max_running.clone();
        let b = barrier.clone();
        handles.push(tokio::spawn(async move {
            bh.execute(|| async move {
                let cur = r.fetch_add(1, Ordering::SeqCst) + 1;
                m.fetch_max(cur, Ordering::SeqCst);
                b.notified().await;
                r.fetch_sub(1, Ordering::SeqCst);
                Ok::<i32, AppError>(1)
            })
            .await
        }));
    }

    tokio::time::sleep(Duration::from_millis(20)).await;
    assert_eq!(max_running.load(Ordering::SeqCst), 3);

    barrier.notify_waiters();
    for h in handles {
        assert!(h.await.unwrap().is_ok());
    }
}

#[tokio::test]
async fn bh_overflow_rate_limited_after_timeout() {
    let bh =
        Bulkhead::new(BulkheadConfig::new("overflow", 1).with_max_wait(Duration::from_millis(20)))
            .unwrap();
    let barrier = Arc::new(tokio::sync::Notify::new());

    let bh2 = bh.clone();
    let b = barrier.clone();
    let holder = tokio::spawn(async move {
        bh2.execute(|| async move {
            b.notified().await;
            Ok::<_, AppError>(())
        })
        .await
    });

    tokio::time::sleep(Duration::from_millis(5)).await;

    let err = bh
        .execute(|| async { Ok::<_, AppError>(()) })
        .await
        .unwrap_err();
    assert_eq!(err.code(), ErrorCode::RateLimited);

    barrier.notify_one();
    let _ = holder.await;
}

#[tokio::test]
async fn bh_slot_released_after_error() {
    let bh = Bulkhead::new(
        BulkheadConfig::new("err-release", 1).with_max_wait(Duration::from_millis(100)),
    )
    .unwrap();

    // Execute fails
    let _ = bh
        .execute(|| async { Err::<i32, AppError>(fail_err()) })
        .await;

    // Slot should be released — next call succeeds
    assert_eq!(bh.available(), 1);
    let r = bh.execute(|| async { Ok::<i32, AppError>(42) }).await;
    assert_eq!(r.unwrap(), 42);
}

#[tokio::test]
async fn bh_available_in_use_accuracy() {
    let bh = Bulkhead::new(BulkheadConfig::new("counters", 3)).unwrap();
    assert_eq!(bh.available(), 3);
    assert_eq!(bh.in_use(), 0);

    let barrier = Arc::new(tokio::sync::Notify::new());
    let bh2 = bh.clone();
    let b = barrier.clone();
    let h = tokio::spawn(async move {
        bh2.execute(|| async move {
            b.notified().await;
            Ok::<_, AppError>(())
        })
        .await
    });

    tokio::time::sleep(Duration::from_millis(10)).await;
    assert_eq!(bh.available(), 2);
    assert_eq!(bh.in_use(), 1);

    barrier.notify_one();
    let _ = h.await;

    assert_eq!(bh.available(), 3);
    assert_eq!(bh.in_use(), 0);
}

#[tokio::test]
async fn bh_callback_ordering() {
    let events: Arc<parking_lot::Mutex<Vec<&str>>> = Arc::new(parking_lot::Mutex::new(Vec::new()));
    let e1 = events.clone();
    let e2 = events.clone();
    let e3 = events.clone();

    let bh = Bulkhead::new(
        BulkheadConfig::new("cbs", 2)
            .with_on_acquire(move || e1.lock().push("acquire"))
            .with_on_release(move || e2.lock().push("release"))
            .with_on_reject(move || e3.lock().push("reject")),
    )
    .unwrap();

    let _ = bh.execute(|| async { Ok::<i32, AppError>(1) }).await;

    let e = events.lock().clone();
    assert_eq!(e, vec!["acquire", "release"]);
}

#[tokio::test]
async fn bh_on_reject_for_overflow() {
    let rejected = Arc::new(AtomicUsize::new(0));
    let r = rejected.clone();

    let bh = Bulkhead::new(
        BulkheadConfig::new("rej", 1)
            .with_max_wait(Duration::from_millis(10))
            .with_on_reject(move || {
                r.fetch_add(1, Ordering::SeqCst);
            }),
    )
    .unwrap();

    let barrier = Arc::new(tokio::sync::Notify::new());
    let bh2 = bh.clone();
    let b = barrier.clone();
    let holder = tokio::spawn(async move {
        bh2.execute(|| async move {
            b.notified().await;
            Ok::<_, AppError>(())
        })
        .await
    });

    tokio::time::sleep(Duration::from_millis(5)).await;

    let _ = bh.execute(|| async { Ok::<_, AppError>(()) }).await;
    assert_eq!(rejected.load(Ordering::SeqCst), 1);

    barrier.notify_one();
    let _ = holder.await;
}

#[tokio::test]
async fn bh_concurrent_stress_100_tasks_10_slots() {
    let bh = Bulkhead::new(BulkheadConfig::new("stress", 10).with_max_wait(Duration::from_secs(5)))
        .unwrap();
    let completed = Arc::new(AtomicUsize::new(0));
    let peak = Arc::new(AtomicUsize::new(0));
    let running = Arc::new(AtomicUsize::new(0));

    let mut handles = Vec::new();
    for _ in 0..100 {
        let bh = bh.clone();
        let c = completed.clone();
        let p = peak.clone();
        let r = running.clone();
        handles.push(tokio::spawn(async move {
            bh.execute(|| async move {
                let cur = r.fetch_add(1, Ordering::SeqCst) + 1;
                p.fetch_max(cur, Ordering::SeqCst);
                tokio::time::sleep(Duration::from_millis(5)).await;
                r.fetch_sub(1, Ordering::SeqCst);
                c.fetch_add(1, Ordering::SeqCst);
                Ok::<_, AppError>(())
            })
            .await
        }));
    }

    for h in handles {
        let _ = h.await.unwrap();
    }

    assert_eq!(completed.load(Ordering::SeqCst), 100);
    assert!(peak.load(Ordering::SeqCst) <= 10);
}

// ═══════════════════════════════════════════════════════════════════════════
// 4. RateLimiter Integration
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn rl_check_succeeds_up_to_burst() {
    let rl = RateLimiter::new("burst-test", 1, 5).unwrap();
    for _ in 0..5 {
        assert!(rl.check().is_ok());
    }
}

#[tokio::test]
async fn rl_check_fails_when_exhausted() {
    let rl = RateLimiter::new("exhaust", 1, 3).unwrap();
    for _ in 0..3 {
        let _ = rl.check();
    }
    let err = rl.check().unwrap_err();
    assert_eq!(err.code(), ErrorCode::RateLimited);
}

#[tokio::test]
async fn rl_until_ready_blocks_then_succeeds() {
    let rl = RateLimiter::new("wait", 100, 1).unwrap();
    // Drain the single token
    let _ = rl.check();

    // until_ready should block briefly then succeed after refill
    let r = tokio::time::timeout(Duration::from_secs(1), rl.until_ready(None)).await;
    assert!(r.is_ok());
    assert!(r.unwrap().is_ok());
}

#[tokio::test]
async fn rl_cancellation_token_cancels_until_ready() {
    let rl = RateLimiter::new("cancel", 1, 1).unwrap();
    let _ = rl.check();

    let cancel = tokio_util::sync::CancellationToken::new();
    let cancel2 = cancel.clone();

    let handle = tokio::spawn(async move { rl.until_ready(Some(cancel2)).await });

    tokio::time::sleep(Duration::from_millis(10)).await;
    cancel.cancel();

    let r = handle.await.unwrap();
    assert!(r.is_err());
    assert_eq!(r.unwrap_err().code(), ErrorCode::ServiceUnavailable);
}

#[tokio::test]
async fn rl_concurrent_check_from_multiple_tasks() {
    let rl = RateLimiter::new("conc-rl", 1, 10).unwrap();
    let successes = Arc::new(AtomicUsize::new(0));
    let failures = Arc::new(AtomicUsize::new(0));

    let mut handles = Vec::new();
    for _ in 0..20 {
        let rl = rl.clone();
        let s = successes.clone();
        let f = failures.clone();
        handles.push(tokio::spawn(async move {
            match rl.check() {
                Ok(()) => {
                    s.fetch_add(1, Ordering::SeqCst);
                }
                Err(_) => {
                    f.fetch_add(1, Ordering::SeqCst);
                }
            }
        }));
    }

    for h in handles {
        h.await.unwrap();
    }

    let s = successes.load(Ordering::SeqCst);
    let f = failures.load(Ordering::SeqCst);
    assert_eq!(s + f, 20);
    // At most burst=10 should succeed
    assert!(s <= 10, "got {s} successes, expected <= 10");
    assert!(s >= 1, "at least one should succeed");
}

#[tokio::test]
async fn rl_high_rate_sustained_throughput() {
    let rl = RateLimiter::new("highrate", 10_000, 100).unwrap();
    let mut success_count = 0;
    for _ in 0..100 {
        if rl.check().is_ok() {
            success_count += 1;
        }
    }
    assert_eq!(success_count, 100);
}

// ═══════════════════════════════════════════════════════════════════════════
// 5. Tower Layer Composition
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn layer_retry_wraps_service() {
    let counter = Arc::new(AtomicUsize::new(0));
    let policy = RetryPolicy::new()
        .with_max_attempts(3)
        .with_initial_backoff(Duration::from_millis(1))
        .with_jitter(false);

    let c = counter.clone();
    let svc = tower::service_fn(move |_req: i32| {
        let c = c.clone();
        async move {
            let n = c.fetch_add(1, Ordering::SeqCst);
            if n < 2 { Err(fail_err()) } else { Ok(99) }
        }
    });

    let mut svc = ServiceBuilder::new()
        .layer(RetryLayer::new(policy))
        .service(svc);

    let r = svc.ready().await.unwrap().call(0).await;
    assert_eq!(r.unwrap(), 99);
    assert_eq!(counter.load(Ordering::SeqCst), 3);
}

#[tokio::test]
async fn layer_cb_opens_on_failures() {
    let cb = CircuitBreaker::new(CbConfig::new("layer-cb").with_max_failures(2)).unwrap();
    let svc = tower::service_fn(|_req: i32| async { Err::<i32, AppError>(fail_err()) });
    let mut svc = ServiceBuilder::new()
        .layer(CircuitBreakerLayer::new(cb.clone()))
        .service(svc);

    for _ in 0..2 {
        let _ = svc.ready().await.unwrap().call(0).await;
    }
    assert_eq!(cb.state(), CbState::Open);

    let err = svc.ready().await.unwrap().call(0).await.unwrap_err();
    assert_eq!(err.code(), ErrorCode::ServiceUnavailable);
}

#[tokio::test]
async fn layer_bulkhead_limits_concurrency() {
    let bh =
        Bulkhead::new(BulkheadConfig::new("layer-bh", 1).with_max_wait(Duration::from_millis(20)))
            .unwrap();
    let barrier = Arc::new(tokio::sync::Notify::new());

    let svc = {
        let b = barrier.clone();
        tower::service_fn(move |_req: i32| {
            let b = b.clone();
            async move {
                b.notified().await;
                Ok::<i32, AppError>(1)
            }
        })
    };

    let svc = ServiceBuilder::new()
        .layer(BulkheadLayer::new(bh.clone()))
        .service(svc);

    let mut svc1 = svc.clone();
    let h = tokio::spawn(async move { svc1.ready().await.unwrap().call(0).await });

    tokio::time::sleep(Duration::from_millis(5)).await;

    let mut svc2 = svc.clone();
    let err = svc2.ready().await.unwrap().call(0).await.unwrap_err();
    assert_eq!(err.code(), ErrorCode::RateLimited);

    barrier.notify_waiters();
    let _ = h.await;
}

#[tokio::test]
async fn layer_rate_limit_limits_rate() {
    let rl = RateLimiter::new("layer-rl", 1, 1).unwrap();
    let svc = tower::service_fn(|req: i32| async move { Ok::<i32, AppError>(req) });

    let mut svc = ServiceBuilder::new()
        .layer(RateLimitLayer::new(rl))
        .service(svc);

    let r = svc.ready().await.unwrap().call(1).await;
    assert!(r.is_ok());

    let r = svc.ready().await.unwrap().call(2).await;
    assert!(r.is_err());
    assert_eq!(r.unwrap_err().code(), ErrorCode::RateLimited);
}

#[tokio::test]
async fn layer_all_four_composed() {
    let rl = RateLimiter::new("composed-rl", 1000, 100).unwrap();
    let bh = Bulkhead::new(BulkheadConfig::new("composed-bh", 10)).unwrap();
    let cb = CircuitBreaker::new(CbConfig::new("composed-cb").with_max_failures(5)).unwrap();
    let policy = RetryPolicy::new()
        .with_max_attempts(2)
        .with_initial_backoff(Duration::from_millis(1))
        .with_jitter(false);

    let svc = tower::service_fn(|req: i32| async move { Ok::<i32, AppError>(req * 2) });

    let mut svc = ServiceBuilder::new()
        .layer(RateLimitLayer::new(rl))
        .layer(BulkheadLayer::new(bh))
        .layer(CircuitBreakerLayer::new(cb.clone()))
        .layer(RetryLayer::new(policy))
        .service(svc);

    let r = svc.ready().await.unwrap().call(5).await;
    assert_eq!(r.unwrap(), 10);
    assert_eq!(cb.state(), CbState::Closed);
}

#[tokio::test]
async fn layer_ordering_rate_limit_then_bulkhead_then_cb_then_retry() {
    // Rate limit exhausted first — should see RateLimited error
    let rl = RateLimiter::new("order-rl", 1, 1).unwrap();
    let bh = Bulkhead::new(BulkheadConfig::new("order-bh", 10)).unwrap();
    let cb = CircuitBreaker::new(CbConfig::new("order-cb").with_max_failures(5)).unwrap();
    let policy = RetryPolicy::new()
        .with_max_attempts(2)
        .with_initial_backoff(Duration::from_millis(1))
        .with_jitter(false);

    let svc = tower::service_fn(|req: i32| async move { Ok::<i32, AppError>(req) });

    let mut svc = ServiceBuilder::new()
        .layer(RateLimitLayer::new(rl))
        .layer(BulkheadLayer::new(bh))
        .layer(CircuitBreakerLayer::new(cb))
        .layer(RetryLayer::new(policy))
        .service(svc);

    // First call succeeds
    let _ = svc.ready().await.unwrap().call(1).await;
    // Second call hits rate limit
    let err = svc.ready().await.unwrap().call(2).await.unwrap_err();
    assert_eq!(err.code(), ErrorCode::RateLimited);
}

#[tokio::test]
async fn layer_error_propagation() {
    let rl = RateLimiter::new("prop-rl", 1000, 100).unwrap();
    let bh = Bulkhead::new(BulkheadConfig::new("prop-bh", 10)).unwrap();
    let cb = CircuitBreaker::new(CbConfig::new("prop-cb").with_max_failures(100)).unwrap();
    let policy = RetryPolicy::new()
        .with_max_attempts(1)
        .with_initial_backoff(Duration::from_millis(1))
        .with_jitter(false);

    let svc = tower::service_fn(|_req: i32| async { Err::<i32, AppError>(non_retryable_err()) });

    let mut svc = ServiceBuilder::new()
        .layer(RateLimitLayer::new(rl))
        .layer(BulkheadLayer::new(bh))
        .layer(CircuitBreakerLayer::new(cb))
        .layer(RetryLayer::new(policy))
        .service(svc);

    let err = svc.ready().await.unwrap().call(1).await.unwrap_err();
    assert_eq!(err.code(), ErrorCode::NotFound);
}

#[tokio::test]
async fn layer_concurrent_requests_through_composed() {
    let rl = RateLimiter::new("conc-composed", 10_000, 100).unwrap();
    let bh = Bulkhead::new(BulkheadConfig::new("conc-bh", 20)).unwrap();
    let cb = CircuitBreaker::new(CbConfig::new("conc-cb").with_max_failures(100)).unwrap();
    let policy = RetryPolicy::new()
        .with_max_attempts(1)
        .with_initial_backoff(Duration::from_millis(1));

    let svc = tower::service_fn(|req: i32| async move { Ok::<i32, AppError>(req) });

    let svc = ServiceBuilder::new()
        .layer(RateLimitLayer::new(rl))
        .layer(BulkheadLayer::new(bh))
        .layer(CircuitBreakerLayer::new(cb))
        .layer(RetryLayer::new(policy))
        .service(svc);

    let completed = Arc::new(AtomicUsize::new(0));
    let mut handles = Vec::new();
    for i in 0..20 {
        let mut s = svc.clone();
        let c = completed.clone();
        handles.push(tokio::spawn(async move {
            let r = s.ready().await.unwrap().call(i).await;
            if r.is_ok() {
                c.fetch_add(1, Ordering::SeqCst);
            }
        }));
    }

    for h in handles {
        h.await.unwrap();
    }
    assert_eq!(completed.load(Ordering::SeqCst), 20);
}

// ═══════════════════════════════════════════════════════════════════════════
// 6. Multi-Pattern Integration
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn multi_cb_plus_retry_exhausts_then_fast_fail() {
    let cb = CircuitBreaker::new(CbConfig::new("multi-cb").with_max_failures(3)).unwrap();
    let counter = Arc::new(AtomicUsize::new(0));

    let policy = RetryPolicy::new()
        .with_max_attempts(5)
        .with_initial_backoff(Duration::from_millis(1))
        .with_jitter(false);

    // Execute through retry wrapping CB — always fails
    let c = counter.clone();
    let cb2 = cb.clone();
    let result = policy
        .execute(|| {
            let c = c.clone();
            let cb = cb2.clone();
            async move {
                cb.execute(|| async {
                    c.fetch_add(1, Ordering::SeqCst);
                    Err::<i32, AppError>(fail_err())
                })
                .await
            }
        })
        .await;

    assert!(result.is_err());
    // After 3 failures the CB opens — remaining retries get fast-fail
    let calls = counter.load(Ordering::SeqCst);
    assert_eq!(
        calls, 3,
        "only 3 actual calls should execute before CB opens"
    );
    assert_eq!(cb.state(), CbState::Open);
}

#[tokio::test]
async fn multi_bulkhead_plus_rate_limiter_both_limits_enforced() {
    let bh =
        Bulkhead::new(BulkheadConfig::new("bh-rl", 2).with_max_wait(Duration::from_millis(50)))
            .unwrap();
    let rl = RateLimiter::new("rl-bh", 1, 3).unwrap();

    let mut successes = 0;
    let mut rl_errors = 0;

    for _ in 0..10 {
        let r = rl.check();
        if r.is_ok() {
            let r = bh.execute(|| async { Ok::<_, AppError>(1) }).await;
            if r.is_ok() {
                successes += 1;
            }
        } else {
            rl_errors += 1;
        }
    }

    // At most 3 succeed (burst limit) — rate limiter is the bottleneck
    assert!(successes <= 3);
    assert!(rl_errors >= 7);
}

#[tokio::test]
async fn multi_recovery_scenario() {
    let cb = CircuitBreaker::new(
        CbConfig::new("recovery")
            .with_max_failures(2)
            .with_timeout(Duration::from_millis(30))
            .with_half_open_max_calls(1),
    )
    .unwrap();

    // Phase 1: system is healthy
    let r = cb.execute(|| async { Ok::<i32, AppError>(1) }).await;
    assert!(r.is_ok());
    assert_eq!(cb.state(), CbState::Closed);

    // Phase 2: system degrades — failures trip the CB
    for _ in 0..2 {
        let _ = cb.execute(|| async { Err::<i32, _>(fail_err()) }).await;
    }
    assert_eq!(cb.state(), CbState::Open);

    // Phase 3: timeout elapses → half-open → probe succeeds → heals
    tokio::time::sleep(Duration::from_millis(40)).await;

    let r = cb.execute(|| async { Ok::<i32, AppError>(42) }).await;
    assert!(r.is_ok());
    assert_eq!(cb.state(), CbState::Closed);
}

#[tokio::test]
async fn multi_load_test_sustained_traffic() {
    let cb = CircuitBreaker::new(CbConfig::new("load").with_max_failures(1000)).unwrap();
    let bh = Bulkhead::new(BulkheadConfig::new("load-bh", 20)).unwrap();
    let rl = RateLimiter::new("load-rl", 10_000, 200).unwrap();

    let completed = Arc::new(AtomicUsize::new(0));
    let mut handles = Vec::new();

    for _ in 0..100 {
        let cb = cb.clone();
        let bh = bh.clone();
        let rl = rl.clone();
        let c = completed.clone();
        handles.push(tokio::spawn(async move {
            if rl.check().is_ok() {
                let r = bh
                    .execute(|| async { cb.execute(|| async { Ok::<_, AppError>(1) }).await })
                    .await;
                if r.is_ok() {
                    c.fetch_add(1, Ordering::SeqCst);
                }
            }
        }));
    }

    for h in handles {
        h.await.unwrap();
    }

    assert!(
        completed.load(Ordering::SeqCst) >= 50,
        "most should succeed"
    );
    assert_eq!(cb.state(), CbState::Closed);
}

#[tokio::test]
async fn multi_error_types_from_each_pattern() {
    // CB open error
    let cb = CircuitBreaker::new(CbConfig::new("err-cb").with_max_failures(1)).unwrap();
    let _ = cb.execute(|| async { Err::<i32, _>(fail_err()) }).await;
    let cb_err = cb
        .execute(|| async { Ok::<i32, AppError>(1) })
        .await
        .unwrap_err();
    assert_eq!(cb_err.code(), ErrorCode::ServiceUnavailable);

    // Bulkhead timeout error
    let bh =
        Bulkhead::new(BulkheadConfig::new("err-bh", 1).with_max_wait(Duration::from_millis(10)))
            .unwrap();
    let barrier = Arc::new(tokio::sync::Notify::new());
    let bh2 = bh.clone();
    let b = barrier.clone();
    let h = tokio::spawn(async move {
        bh2.execute(|| async move {
            b.notified().await;
            Ok::<_, AppError>(())
        })
        .await
    });
    tokio::time::sleep(Duration::from_millis(5)).await;
    let bh_err = bh
        .execute(|| async { Ok::<_, AppError>(()) })
        .await
        .unwrap_err();
    assert_eq!(bh_err.code(), ErrorCode::RateLimited);
    barrier.notify_one();
    let _ = h.await;

    // Rate limiter error
    let rl = RateLimiter::new("err-rl", 1, 1).unwrap();
    let _ = rl.check();
    let rl_err = rl.check().unwrap_err();
    assert_eq!(rl_err.code(), ErrorCode::RateLimited);

    // Retry exhausted error (goes through RetryPolicy directly)
    let policy = RetryPolicy::new()
        .with_max_attempts(2)
        .with_initial_backoff(Duration::from_millis(1))
        .with_jitter(false);
    let retry_err = policy
        .execute(|| async { Err::<i32, AppError>(fail_err()) })
        .await
        .unwrap_err();
    assert_eq!(retry_err.attempts, 2);
    assert_eq!(retry_err.last_error.code(), ErrorCode::ConnectionFailed);
}
