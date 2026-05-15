//! [`tower::Layer`] adapters for resilience capabilities.
//!
//! Each adapter lives with the capability it wraps, while this module remains
//! the stable aggregation point for Tower integration:
//!
//! ```ignore
//! use tower::ServiceBuilder;
//! use rskit_resilience::layers::{CircuitBreakerLayer, RetryLayer};
//!
//! let svc = ServiceBuilder::new()
//!     .layer(CircuitBreakerLayer::new(cb))
//!     .layer(RetryLayer::new(policy))
//!     .service(my_base_service);
//! ```

mod bulkhead;
mod circuit_breaker;
mod rate_limit;
mod retry;
mod timeout;

pub use bulkhead::{BulkheadLayer, BulkheadService};
pub use circuit_breaker::{CircuitBreakerLayer, CircuitBreakerService};
pub use rate_limit::{RateLimitLayer, RateLimitService};
pub use retry::{RetryLayer, RetryService};
pub use timeout::{TimeoutLayer, TimeoutService};
