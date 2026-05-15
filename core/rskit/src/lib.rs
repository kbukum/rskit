//! `rskit` — production Rust toolkit.
//!
//! This crate is a thin facade that re-exports all `rskit-*` sub-crates from a
//! single namespace. Feature flags control optional transports:
//!
//! | Feature | Extra crate |
//! |---------|-------------|
//! | `server` | `rskit-server` (service-facing HTTP + lifecycle) |
//! | `grpc`   | `rskit-grpc` (aligned gRPC client/server transport) |
//! | `http`   | `rskit-http` (axum transport details) |
//! | `auth`   | `rskit-auth` (JWT, OIDC, password) |
//! | `di`     | `rskit-di` (dependency injection) |
//! | `database` | `rskit-database` (sqlx) |
//! | `cache`  | `rskit-cache` (Redis) |
//! | `messaging` | `rskit-messaging` (core abstractions + in-memory backend) |
//! | `messaging-kafka` | `Kafka` messaging adapter |
//! | `messaging-nats` | NATS messaging adapter |
//! | `messaging-rabbitmq` | `RabbitMQ` messaging adapter |
//! | `observability` | `rskit-observability` (OpenTelemetry) |
//! | `authz`  | `rskit-authz` (RBAC/ABAC) |
//! | `security` | `rskit-security` (security headers) |
//! | `discovery` | `rskit-discovery` (service discovery) |
//! | `testutil` | `rskit-testutil` (test helpers) |
//! | `sse`    | `rskit-sse` (Server-Sent Events) |
//! | `dag`    | `rskit-dag` (DAG orchestration) |
//! | `chain`  | `rskit-chain` (sequential execution) |
//! | `process` | `rskit-process` (subprocess execution) |
//! | `stateful` | `rskit-stateful` (stateful accumulators) |
//! | `genai` | `rskit-ai` (shared `GenAI` vocabulary) |
//! | `llm`    | `rskit-llm` (LLM providers) |
//! | `storage` | `rskit-storage` (File I/O, storage) |
//! | `media`  | `rskit-media` (media types, pipeline) |
//! | `media-ffmpeg` | `rskit-media-ffmpeg` (`FFmpeg` backend) |
//! | `media-image`  | `rskit-media-image` (image processing) |
//! | `media-full` | ffmpeg + image backends |
//! | `storage-s3` | S3 storage backend |
//! | `storage-gcs` | GCS storage backend |
//! | `workload` | `rskit-workload` (workload management) |
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

// ── Mockable clock abstraction ────────────────────────────────────────────────

pub mod clock;
pub use clock::{Clock, SystemClock};

// NOTE(#64 RS-ME-29): Prefer `impl Trait` in function arguments over `Box<dyn Trait>`
// for zero-cost abstraction in hot paths. Use `Box<dyn Trait>` only when
// type erasure is genuinely needed (e.g., storing heterogeneous types in a collection).

// ── Always-on sub-crate facades ──────────────────────────────────────────────

/// Error types, `ErrorCode`, `AppError`, `AppResult`.
pub use rskit_errors as errors;

/// Config loading (TOML + env).
pub use rskit_config as config;

/// `tracing` subscriber setup.
pub use rskit_logging as logging;

/// Retry, circuit breaker, bulkhead, rate limiter — and tower layers.
pub use rskit_resilience as resilience;

/// Provider traits + tower bridge.
pub use rskit_provider as provider;

/// `futures::Stream` extension trait + operators.
pub use rskit_pipeline as pipeline;

/// App lifecycle orchestration.
pub use rskit_bootstrap as bootstrap;

/// Component lifecycle primitives: `Component`, `Registry`, health, and state.
pub use rskit_component as component;

/// Worker pool, `Handler` trait, typed events.
pub use rskit_worker as worker;

/// Fluent field-level validation.
pub use rskit_validation as validation;

/// Sequential chain execution utilities.
#[cfg(feature = "chain")]
pub use rskit_chain as chain;

/// Safe subprocess execution helpers.
#[cfg(feature = "process")]
pub use rskit_process as process;

/// Stateful accumulators and keyed managers.
#[cfg(feature = "stateful")]
pub use rskit_stateful as stateful;

// ── Feature-gated sub-crate facades ──────────────────────────────────────────

/// gRPC server component (opt-in via `server` feature).
#[cfg(feature = "server")]
pub use rskit_server as server;

/// Aligned gRPC transport namespace (opt-in via `grpc` feature).
#[cfg(feature = "grpc")]
pub use rskit_grpc as grpc;

/// Axum transport details used by `rskit-server`.
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

/// Message broker abstractions and in-memory backend.
#[cfg(feature = "messaging")]
pub use rskit_messaging as messaging;

/// Kafka messaging adapter.
#[cfg(feature = "messaging-kafka")]
pub use rskit_messaging_kafka as messaging_kafka;

/// NATS messaging adapter.
#[cfg(feature = "messaging-nats")]
pub use rskit_messaging_nats as messaging_nats;

/// RabbitMQ messaging adapter.
#[cfg(feature = "messaging-rabbitmq")]
pub use rskit_messaging_rabbitmq as messaging_rabbitmq;

/// OpenTelemetry tracing, metrics, and context propagation.
#[cfg(feature = "observability")]
pub use rskit_observability as observability;

/// RBAC and ABAC authorization engine.
#[cfg(feature = "authz")]
pub use rskit_authz as authz;

/// Security headers and transport security policy.
#[cfg(feature = "security")]
pub use rskit_security as security;

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

/// Shared GenAI vocabulary.
#[cfg(feature = "genai")]
pub use rskit_ai as genai;

/// LLM provider abstractions for OpenAI and Anthropic.
#[cfg(feature = "llm")]
pub use rskit_llm as llm;

/// Model-serving runtime inference abstractions.
#[cfg(feature = "inference")]
pub use rskit_inference as inference;

/// Triton KServe v2 HTTP inference adapter.
#[cfg(feature = "inference-triton")]
pub use rskit_inference_triton as inference_triton;

/// vLLM raw REST inference adapter skeleton.
#[cfg(feature = "inference-vllm")]
pub use rskit_inference_vllm as inference_vllm;

/// Hugging Face TGI REST inference adapter skeleton.
#[cfg(feature = "inference-tgi")]
pub use rskit_inference_tgi as inference_tgi;

/// Prompt templates and output schema contracts.
#[cfg(feature = "prompt")]
pub use rskit_ai::prompt;

/// Skill manifests, loaders, registries, and verification contracts.
#[cfg(feature = "skill")]
pub use rskit_skill as skill;

/// File I/O, storage backends, MIME detection, temp files.
#[cfg(feature = "storage")]
pub use rskit_storage as storage;

/// Amazon S3 and S3-compatible (MinIO, LocalStack) storage backend.
#[cfg(feature = "storage-s3")]
pub use rskit_storage_s3 as storage_s3;

/// Google Cloud Storage backend.
#[cfg(feature = "storage-gcs")]
pub use rskit_storage_gcs as storage_gcs;

/// Media types, codec/format registry, pipeline builder.
#[cfg(feature = "media")]
pub use rskit_media as media;

/// FFmpeg CLI backend for video/audio processing.
#[cfg(feature = "media-ffmpeg")]
pub use rskit_media_ffmpeg as media_ffmpeg;

/// Native image processing backend using the `image` crate.
#[cfg(feature = "media-image")]
pub use rskit_media_image as media_image;

/// Workload configuration and orchestration primitives.
#[cfg(feature = "workload")]
pub use rskit_workload as workload;

/// CLI helpers: progress bars, cancellation tokens, output formatting.
#[cfg(feature = "cli")]
pub use rskit_cli as cli;

/// Git operations: repository management, commits, branches, tags, diffs.
#[cfg(feature = "git")]
pub use rskit_git as git;

/// Dataset collection: sources, transforms, targets, manifest caching.
#[cfg(feature = "dataset")]
pub use rskit_dataset as dataset;

/// ML benchmarking: evaluators, metrics, reports, visualization.
#[cfg(feature = "bench")]
pub use rskit_bench as bench;

// ── Convenience re-exports at root ──────────────────────────────────────────

pub use rskit_bootstrap::{App, AppBuilder};
pub use rskit_component::{
    Component, Health, HealthStatus, LazyComponent, Registry, RegistryConfig, State, StopResult,
};
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
