//! Fault-tolerance primitives: retry, circuit breaker, bulkhead, and rate limiter.

#![warn(missing_docs)]

/// Semaphore-based concurrency limiter.
pub mod bulkhead;
/// Asynchronous circuit breaker with closed / open / half-open states.
pub mod circuit_breaker;
/// Token-bucket rate limiter backed by `governor`.
pub mod rate_limiter;
/// Exponential back-off retry policy.
pub mod retry;
/// [`tower::Layer`] wrappers for each resilience primitive.
pub mod layers;

pub use bulkhead::{Bulkhead, BulkheadConfig};
pub use circuit_breaker::{CbConfig, CbState, CircuitBreaker};
pub use rate_limiter::RateLimiter;
pub use retry::{RetryError, RetryPolicy};

pub use layers::{BulkheadLayer, CircuitBreakerLayer, RateLimitLayer, RetryLayer};
