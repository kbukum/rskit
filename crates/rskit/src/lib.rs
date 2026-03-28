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
