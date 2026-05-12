//! Middleware stack for message handlers.
//!
//! Each middleware implements [`HandlerMiddleware`](crate::HandlerMiddleware)
//! and wraps a handler with cross-cutting concerns such as retries, metrics,
//! tracing, deduplication, dead-letter routing, and circuit breaking.

/// Circuit breaker middleware backed by `rskit-resilience`.
pub mod circuit_breaker;
/// Dead-letter queue routing for failed messages.
pub mod deadletter;
/// Message deduplication based on the `message-id` header.
pub mod dedup;
/// Handler-level metrics instrumentation.
pub mod metrics;
/// Retry with exponential backoff.
pub mod retry;
/// Middleware stack builder for composing handler pipelines.
pub mod stack;
/// Tracing span instrumentation.
#[allow(clippy::module_inception)]
pub mod tracing;

pub use self::tracing::tracing_middleware;
pub use circuit_breaker::{CircuitBreakerConfig, circuit_breaker};
pub use deadletter::{DeadLetterConfig, DeadLetterEnvelope, DeadLetterPayloadSummary, dead_letter};
pub use dedup::{DedupConfig, dedup};
pub use metrics::instrument;
pub use retry::{RetryConfig, retry};
pub use stack::StackBuilder;
