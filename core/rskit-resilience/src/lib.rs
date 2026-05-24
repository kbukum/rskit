//! Fault-tolerance primitives: retry, circuit breaker, bulkhead, and rate limiter.

#![warn(missing_docs)]

/// Semaphore-based concurrency limiter.
pub mod bulkhead;
/// Asynchronous circuit breaker with closed / open / half-open states.
pub mod circuit_breaker;
/// [`tower::Layer`] wrappers for each resilience primitive.
pub mod layers;
/// High-level composition API for resilience primitives.
pub mod policy;
/// Token-bucket rate limiter backed by `governor`.
pub mod rate_limiter;
/// Exponential, constant, and linear back-off retry policies.
pub mod retry;

pub use bulkhead::{Bulkhead, BulkheadConfig};
pub use circuit_breaker::{CbConfig, CbState, CircuitBreaker};
pub use policy::Policy;
pub use rate_limiter::{RateLimiter, RateLimiterConfig};
pub use retry::{
    BackoffKind, ConstantBackoff, LinearBackoff, RetryError, RetryPolicy, RetryPreset,
};

pub use layers::{BulkheadLayer, CircuitBreakerLayer, RateLimitLayer, RetryLayer, TimeoutLayer};
