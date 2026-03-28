//! # Hello Service
//!
//! A minimal end-to-end example showing rskit's lifecycle, error handling,
//! and resilience primitives working together.
//!
//! Run with:
//! ```sh
//! cargo run --example hello_service -p rskit --features full
//! ```

use rskit_errors::{AppError, AppResult, ErrorCode};
use rskit_logging::{init_logging_env, info, warn};
use rskit_resilience::{CbConfig, CircuitBreaker, RetryPolicy};
use std::time::Duration;

// ---------------------------------------------------------------------------
// A toy downstream "service" that fails twice before succeeding
// ---------------------------------------------------------------------------
static CALL_COUNT: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

async fn call_downstream() -> AppResult<String> {
    let n = CALL_COUNT.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
    if n < 2 {
        warn!("downstream call failed (attempt {})", n + 1);
        Err(AppError::new(ErrorCode::ServiceUnavailable, "not ready yet"))
    } else {
        info!("downstream call succeeded on attempt {}", n + 1);
        Ok("pong".to_string())
    }
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------
#[tokio::main]
async fn main() -> AppResult<()> {
    let _guard = init_logging_env();

    let cb = CircuitBreaker::new(CbConfig::new("downstream").with_max_failures(5));
    let retry = RetryPolicy::new()
        .with_max_attempts(5)
        .with_initial_backoff(Duration::from_millis(50));

    let result = retry
        .execute(|| {
            let cb = cb.clone();
            async move { cb.execute(|| call_downstream()).await }
        })
        .await
        .map_err(|e| e.last_error)?;

    info!("Got: {result}");
    Ok(())
}
