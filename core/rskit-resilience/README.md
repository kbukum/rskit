# rskit-resilience — Retry, Circuit Breaker, Bulkhead, Rate Limiter, Timeout

Production-grade resilience primitives with Tower layer integration.

[![CI](https://github.com/kbukum/rskit/actions/workflows/ci.yml/badge.svg)](https://github.com/kbukum/rskit/actions/workflows/ci.yml) [![crates.io](https://img.shields.io/crates/v/rskit-resilience.svg)](https://crates.io/crates/rskit-resilience) [![docs.rs](https://docs.rs/rskit-resilience/badge.svg)](https://docs.rs/rskit-resilience) [![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/kbukum/rskit/blob/main/LICENSE) [![MSRV: 1.91](https://img.shields.io/badge/MSRV-1.91-orange.svg)](https://github.com/kbukum/rskit/blob/main/core/Cargo.toml)

## Features

- Exponential backoff + jitter retry via `RetryPolicy`, bounded by attempts and elapsed time
- Three-state circuit breaker (`Closed` / `Open` / `HalfOpen`) backed by `parking_lot`
- Semaphore-based bulkhead for concurrency limiting
- `governor`-backed rate limiter
- Tower layers: `RetryLayer`, `CircuitBreakerLayer`, `BulkheadLayer`, `RateLimitLayer`,
  `TimeoutLayer`

## Usage

```toml
[dependencies]
rskit-resilience = "0.2.0-alpha.1"
rskit-errors = "0.2.0-alpha.3"
```

```rust
use rskit_errors::AppResult;
use rskit_resilience::{RetryPolicy, CircuitBreaker, CbConfig};
use std::time::Duration;

async fn call_with_resilience() -> AppResult<()> {
    let cb = CircuitBreaker::new(CbConfig::new("my-service"))?;
    let retry = RetryPolicy::new()
        .with_max_attempts(3)
        .with_initial_backoff(Duration::from_millis(100))
        .with_max_elapsed_time(Duration::from_secs(5));

    let _result = retry.execute(|| async {
        cb.execute(|| async { call_downstream().await }).await
    }).await.map_err(|err| {
        let attempts = err.attempts as u64;
        err.last_error.with_detail("retry_attempts", attempts)
    })?;

    Ok(())
}
```

## See Also

[Main repository README](https://github.com/kbukum/rskit)
