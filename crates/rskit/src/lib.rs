//! `rskit` — production Rust toolkit.
//!
//! This crate is a thin facade that re-exports all `rskit-*` sub-crates from a
//! single namespace. Feature flags control optional transports:
//!
//! | Feature | Extra crate |
//! |---------|-------------|
//! | `server` | `rskit-server` (tonic gRPC) |
//! | `http`   | `rskit-http` (axum HTTP) |
//! | `auth`   | `rskit-auth` (JWT, OIDC, password) |
//! | `di`     | `rskit-di` (dependency injection) |
//! | `database` | `rskit-database` (sqlx) |
//! | `cache`  | `rskit-cache` (Redis) |
//! | `messaging` | `rskit-messaging` (Kafka) |
//! | `observability` | `rskit-observability` (OpenTelemetry) |
//! | `authz`  | `rskit-authz` (RBAC/ABAC) |
//! | `discovery` | `rskit-discovery` (service discovery) |
//! | `testutil` | `rskit-testutil` (test helpers) |
//! | `sse`    | `rskit-sse` (Server-Sent Events) |
//! | `dag`    | `rskit-dag` (DAG orchestration) |
//! | `llm`    | `rskit-llm` (LLM providers) |
//! | `file`   | `rskit-file` (File I/O, storage) |
//! | `media`  | `rskit-media` (media types, pipeline) |
//! | `media-ffmpeg` | `rskit-media-ffmpeg` (FFmpeg backend) |
//! | `media-image`  | `rskit-media-image` (image processing) |
//! | `media-full` | ffmpeg + image backends |
//! | `file-s3` | S3 storage backend |
//! | `file-gcs` | GCS storage backend |
//! | `cli`    | `rskit-cli` (CLI helpers) |
//! | `dataset` | `rskit-dataset` (dataset collection) |
//! | `bench`  | `rskit-bench` (ML benchmarking) |
//! | `full`   | all features |
//!
//! # Quick start
//!
//! ```toml
//! [dependencies]
//! rskit = { version = "0.1", features = ["full"] }
//! ```

#![warn(missing_docs)]

// ── Supervised task helper ────────────────────────────────────────────────────

pub mod task;
pub use task::{SupervisedTask, supervise};

// ── Generic typed registry ────────────────────────────────────────────────────

pub mod registry;
pub use registry::TypedRegistry;

// ── Always-on sub-crate facades ──────────────────────────────────────────────

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

/// Fluent field-level validation.
pub use rskit_validation as validation;

// ── Feature-gated sub-crate facades ──────────────────────────────────────────

/// gRPC server component (opt-in via `server` feature).
#[cfg(feature = "server")]
pub use rskit_server as server;

/// Axum HTTP server with CORS, request-ID, and Component lifecycle.
#[cfg(feature = "http")]
pub use rskit_http as http;

/// Async HTTP client with auth, retries, and error handling.
#[cfg(feature = "httpclient")]
pub use rskit_httpclient as httpclient;

/// JWT, OIDC, password hashing, and request-context auth helpers.
#[cfg(feature = "auth")]
pub use rskit_auth as auth;

/// Lightweight runtime dependency injection container.
#[cfg(feature = "di")]
pub use rskit_di as di;

/// sqlx-based async database pool with repository pattern.
#[cfg(feature = "database")]
pub use rskit_database as database;

/// Redis client with typed store and Component lifecycle.
#[cfg(feature = "cache")]
pub use rskit_cache as cache;

/// Message broker abstractions (Kafka, in-memory).
#[cfg(feature = "messaging")]
pub use rskit_messaging as messaging;

/// OpenTelemetry tracing, metrics, and context propagation.
#[cfg(feature = "observability")]
pub use rskit_observability as observability;

/// RBAC and ABAC authorization engine.
#[cfg(feature = "authz")]
pub use rskit_authz as authz;

/// Service discovery with load balancing strategies.
#[cfg(feature = "discovery")]
pub use rskit_discovery as discovery;

/// Test utilities, mock providers, and assertion helpers.
#[cfg(feature = "testutil")]
pub use rskit_testutil as testutil;

/// Server-Sent Events bus with axum integration.
#[cfg(feature = "sse")]
pub use rskit_sse as sse;

/// DAG task orchestrator with parallel execution.
#[cfg(feature = "dag")]
pub use rskit_dag as dag;

/// LLM provider abstractions for OpenAI and Anthropic.
#[cfg(feature = "llm")]
pub use rskit_llm as llm;

/// File I/O, storage backends, MIME detection, temp files.
#[cfg(feature = "file")]
pub use rskit_file as file;

/// Amazon S3 and S3-compatible (MinIO, LocalStack) storage backend.
#[cfg(feature = "file-s3")]
pub use rskit_file_s3 as file_s3;

/// Media types, codec/format registry, pipeline builder.
#[cfg(feature = "media")]
pub use rskit_media as media;

/// FFmpeg CLI backend for video/audio processing.
#[cfg(feature = "media-ffmpeg")]
pub use rskit_media_ffmpeg as media_ffmpeg;

/// Native image processing backend using the `image` crate.
#[cfg(feature = "media-image")]
pub use rskit_media_image as media_image;

/// CLI helpers: progress bars, cancellation tokens, output formatting.
#[cfg(feature = "cli")]
pub use rskit_cli as cli;

/// Dataset collection: sources, transforms, targets, manifest caching.
#[cfg(feature = "dataset")]
pub use rskit_dataset as dataset;

/// ML benchmarking: evaluators, metrics, reports, visualization.
#[cfg(feature = "bench")]
pub use rskit_bench as bench;

// ── Convenience re-exports at root ──────────────────────────────────────────

pub use rskit_bootstrap::{App, AppBuilder, Component, Health, HealthStatus, Registry};
pub use rskit_config::{AppConfig, ConfigLoader, ServiceConfig};
pub use rskit_errors::{AppError, AppResult, ErrorCode};
pub use rskit_logging::{LoggingGuard, init_logging, init_logging_env};
pub use rskit_provider::traits::{Provider, RequestResponse, Sink};
pub use rskit_resilience::{CircuitBreaker, RateLimiter, RetryPolicy};
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
        use rskit_errors::AppResult;
        use rskit_worker::Event;
        use std::sync::Arc;
        use tokio::sync::mpsc;
        use tokio_util::sync::CancellationToken;

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
