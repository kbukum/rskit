# rskit-resilience — Retry, Circuit Breaker, Bulkhead, Rate Limiter, Timeout

Production-grade resilience primitives with Tower layer integration.

[![CI](https://github.com/kbukum/rskit/actions/workflows/ci.yml/badge.svg)](https://github.com/kbukum/rskit/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/rskit-resilience.svg)](https://crates.io/crates/rskit-resilience)
[![docs.rs](https://docs.rs/rskit-resilience/badge.svg)](https://docs.rs/rskit-resilience)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![MSRV: 1.85](https://img.shields.io/badge/MSRV-1.85-orange.svg)](https://blog.rust-lang.org/2025/02/20/Rust-1.85.0.html)

## Features

- Exponential backoff + jitter retry via `RetryPolicy`, bounded by attempts and elapsed time
- Three-state circuit breaker (`Closed` / `Open` / `HalfOpen`) backed by `parking_lot`
- Semaphore-based bulkhead for concurrency limiting
- `governor`-backed rate limiter
- Tower layers: `RetryLayer`, `CircuitBreakerLayer`, `BulkheadLayer`, `RateLimitLayer`, `TimeoutLayer`

## Usage

```toml
[dependencies]
rskit-resilience = "0.1"
```

```rust
use rskit_resilience::{RetryPolicy, CircuitBreaker, CbConfig};
use std::time::Duration;

let cb = CircuitBreaker::new(CbConfig::new("my-service"));
let retry = RetryPolicy::new()
    .with_max_attempts(3)
    .with_initial_backoff(Duration::from_millis(100))
    .with_max_elapsed_time(Duration::from_secs(5));

let result = retry.execute(|| async {
    cb.execute(|| async { call_downstream().await }).await
}).await?;
```

## See Also

[Main repository README](https://github.com/kbukum/rskit)
