//! `rskit` — production Rust toolkit.
//!
//! This crate is a thin facade that re-exports all `rskit-*` sub-crates from a
//! single namespace. Feature flags control optional transports:
//!
//! | Feature | Extra crate |
//! |---------|-------------|
//! | `server` | `rskit-server` (tonic gRPC) |
//! | `full`   | all features |
//!
//! # Quick start
//!
//! ```toml
//! [dependencies]
//! rskit = { version = "0.1", features = ["full"] }
//! ```

#![warn(missing_docs)]

// ── Sub-crate facades ────────────────────────────────────────────────────────

/// Error types, `ErrorCode`, `AppError`, `AppResult`.
pub use rskit_errors as errors;

/// Config loading (TOML + env).
pub use rskit_config as config;

/// `tracing` subscriber setup.
pub use rskit_logging as logging;

/// Retry, circuit breaker, bulkhead, rate limiter — and tower layers.
pub use rskit_resilience as resilience;

/// Provider traits + tower bridge + middleware.
pub use rskit_provider as provider;

/// `futures::Stream` extension trait + operators.
pub use rskit_pipeline as pipeline;

/// App lifecycle, `Component` trait, `Registry`.
pub use rskit_bootstrap as bootstrap;

/// Worker pool, `Handler` trait, typed events.
pub use rskit_worker as worker;

/// gRPC server component (opt-in via `server` feature).
#[cfg(feature = "server")]
pub use rskit_server as server;

// ── Convenience re-exports at root ──────────────────────────────────────────

pub use rskit_errors::{AppError, AppResult, ErrorCode};
pub use rskit_bootstrap::{App, AppBuilder, Component, Health, HealthStatus, Registry};
pub use rskit_config::{AppConfig, ConfigLoader, ServiceConfig};
pub use rskit_logging::{init_logging, init_logging_env, LoggingGuard};
pub use rskit_resilience::{CircuitBreaker, RateLimiter, RetryPolicy};
pub use rskit_provider::traits::{Provider, RequestResponse, Sink};
pub use rskit_worker::{Handler, Pool, PoolConfig, TaskHandle};

#[cfg(test)]
mod tests {
    /// Verify the root re-exports compile and hold the expected types.
    /// These are compile-time tests — if they build, the facade is wired correctly.
    use super::*;

    #[test]
    fn error_code_accessible_from_facade() {
        let e = AppError::new(ErrorCode::NotFound, "not found");
        assert_eq!(e.code, ErrorCode::NotFound);
        assert!(!e.is_retryable());
    }

    #[test]
    fn retry_policy_accessible_from_facade() {
        let p = RetryPolicy::new().with_max_attempts(2);
        assert_eq!(p.max_attempts, 2);
    }

    #[test]
    fn circuit_breaker_accessible_from_facade() {
        let cb = CircuitBreaker::new(rskit_resilience::CbConfig::new("facade-cb"));
        assert_eq!(cb.state(), rskit_resilience::CbState::Closed);
    }

    #[test]
    fn rate_limiter_accessible_from_facade() {
        let rl = RateLimiter::new("facade-rl", 10, 5);
        assert!(rl.check().is_ok());
    }

    #[tokio::test]
    async fn pool_accessible_from_facade() {
        use std::sync::Arc;
        use tokio::sync::mpsc;
        use tokio_util::sync::CancellationToken;
        use rskit_errors::AppResult;
        use rskit_worker::Event;

        struct EchoHandler;
        #[async_trait::async_trait]
        impl Handler<i32, i32> for EchoHandler {
            async fn handle(
                &self,
                task: i32,
                _emit: mpsc::Sender<Event<i32>>,
                _cancel: CancellationToken,
            ) -> AppResult<i32> {
                Ok(task)
            }
        }

        let pool = Pool::new(Arc::new(EchoHandler), PoolConfig::new("facade-pool"));
        let handle = pool.submit(99).await.unwrap();
        assert_eq!(handle.result().await.unwrap(), 99);
    }

    #[test]
    fn health_types_accessible_from_facade() {
        let h = Health::healthy("svc");
        assert_eq!(h.status, HealthStatus::Healthy);
        assert!(h.is_healthy());
    }
}
