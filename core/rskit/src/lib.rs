//! `rskit` — production Rust toolkit.
//!
//! This crate is a pure facade that re-exports `rskit-*` sub-crates from a
//! single namespace. It contains no implementation logic. Feature flags control
//! optional modules:
//!
//! | Feature | Extra crate |
//! |---------|-------------|
//! | `server` | `rskit-server` (service-facing HTTP + lifecycle) |
//! | `grpc`   | `rskit-grpc` (aligned gRPC client/server transport) |
//! | `encryption` | `rskit-encryption` (encryption helpers) |
//! | `http`   | `rskit-http` (axum transport details) |
//! | `auth`   | `rskit-auth` (JWT, OIDC, password) |
//! | `di`     | `rskit-di` (dependency injection) |
//! | `database` | `rskit-database` (core + memory backend) |
//! | `cache`  | `rskit-cache` (core + memory backend) |
//! | `cache-redis` | Redis cache adapter |
//! | `messaging` | `rskit-messaging` (core abstractions + in-memory backend) |
//! | `messaging-kafka` | `Kafka` messaging adapter |
//! | `messaging-nats` | NATS messaging adapter |
//! | `messaging-rabbitmq` | `RabbitMQ` messaging adapter |
//! | `vectorstore` | `rskit-vectorstore` (core + memory backend) |
//! | `vectorstore-qdrant` | Qdrant vector store adapter |
//! | `observability` | `rskit-observability` (OpenTelemetry) |
//! | `authz`  | `rskit-authz` (RBAC/ABAC) |
//! | `security` | `rskit-security` (TLS/security configuration) |
//! | `discovery` | `rskit-discovery` (service discovery) |
//! | `sse`    | `rskit-sse` (Server-Sent Events) |
//! | `dag`    | `rskit-dag` (DAG orchestration) |
//! | `chain`  | `rskit-chain` (sequential execution) |
//! | `process` | `rskit-process` (subprocess execution) |
//! | `stateful` | `rskit-stateful` (stateful accumulators) |
//! | `genai` | `rskit-ai` (shared `GenAI` vocabulary) |
//! | `llm`    | `rskit-llm` (LLM providers) |
//! | `llm-openai` | `OpenAI` LLM adapter |
//! | `llm-anthropic` | `Anthropic` LLM adapter |
//! | `llm-ollama` | `Ollama` LLM adapter |
//! | `llm-gemini` | `Gemini` LLM adapter |
//! | `embedding` | `rskit-embedding` (embedding abstractions) |
//! | `inference` | `rskit-inference` (model-serving abstractions) |
//! | `tool` | `rskit-tool` (tool contracts) |
//! | `agent` | `rskit-agent` (agent contracts) |
//! | `mcp` | `rskit-mcp` (Model Context Protocol support) |
//! | `storage` | `rskit-storage` (File I/O, storage) |
//! | `media`  | `rskit-media` (media types, pipeline) |
//! | `media-ffmpeg` | `rskit-media-ffmpeg` (`FFmpeg` backend) |
//! | `media-image`  | `rskit-media-image` (image processing) |
//! | `media-audio`  | `rskit-media-audio` (pure Rust audio analysis) |
//! | `media-full` | `FFmpeg` + image + audio backends |
//! | `storage-s3` | `S3` storage backend |
//! | `storage-gcs` | GCS storage backend |
//! | `workload` | `rskit-workload` (workload management) |
//! | `cli`    | `rskit-cli` (CLI helpers) |
//! | `git`    | `rskit-git` (Git automation) |
//! | `dataset` | `rskit-dataset` (dataset collection) |
//! | `bench`  | `rskit-bench` (ML benchmarking) |
//! | `full`   | all features |
//!
//! Test helpers live in `rskit-testutil` and should be added directly as a
//! `dev-dependency`; they are intentionally not part of this production facade.
//!
//! # Quick start
//!
//! ```toml
//! [dependencies]
//! rskit = { version = "0.1", features = ["full"] }
//! ```

#![warn(missing_docs)]

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

/// Encryption helpers (opt-in via `encryption` feature).
#[cfg(feature = "encryption")]
pub use rskit_encryption as encryption;

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

/// Database contracts with in-memory default and adapter registry.
#[cfg(feature = "database")]
pub use rskit_database as database;

/// Cache contracts with in-memory default and adapter registry.
#[cfg(feature = "cache")]
pub use rskit_cache as cache;

/// Redis cache adapter.
#[cfg(feature = "cache-redis")]
pub use rskit_cache_redis as cache_redis;

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

/// Shared TLS and security configuration.
#[cfg(feature = "security")]
pub use rskit_security as security;

/// Service discovery with load balancing strategies.
#[cfg(feature = "discovery")]
pub use rskit_discovery as discovery;

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

/// OpenAI LLM adapter.
#[cfg(feature = "llm-openai")]
pub use rskit_llm_openai as llm_openai;

/// Anthropic LLM adapter.
#[cfg(feature = "llm-anthropic")]
pub use rskit_llm_anthropic as llm_anthropic;

/// Ollama LLM adapter.
#[cfg(feature = "llm-ollama")]
pub use rskit_llm_ollama as llm_ollama;

/// Gemini LLM adapter.
#[cfg(feature = "llm-gemini")]
pub use rskit_llm_gemini as llm_gemini;

/// Embedding provider abstractions and types.
#[cfg(feature = "embedding")]
pub use rskit_embedding as embedding;

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

/// Tool schemas, callable contracts, and metadata wrappers.
#[cfg(feature = "tool")]
pub use rskit_tool as tool;

/// Agent contracts and orchestration primitives.
#[cfg(feature = "agent")]
pub use rskit_agent as agent;

/// Model Context Protocol contracts.
#[cfg(feature = "mcp")]
pub use rskit_mcp as mcp;

/// File I/O, storage backends, MIME detection, temp files.
#[cfg(feature = "storage")]
pub use rskit_storage as storage;

/// Amazon S3 and S3-compatible (MinIO, LocalStack) storage backend.
#[cfg(feature = "storage-s3")]
pub use rskit_storage_s3 as storage_s3;

/// Google Cloud Storage backend.
#[cfg(feature = "storage-gcs")]
pub use rskit_storage_gcs as storage_gcs;

/// Vector store contracts with in-memory default and adapter registry.
#[cfg(feature = "vectorstore")]
pub use rskit_vectorstore as vectorstore;

/// Qdrant vector store adapter.
#[cfg(feature = "vectorstore-qdrant")]
pub use rskit_vectorstore_qdrant as vectorstore_qdrant;

/// Media types, codec/format registry, pipeline builder.
#[cfg(feature = "media")]
pub use rskit_media as media;

/// FFmpeg CLI backend for video/audio processing.
#[cfg(feature = "media-ffmpeg")]
pub use rskit_media_ffmpeg as media_ffmpeg;

/// Native image processing backend using the `image` crate.
#[cfg(feature = "media-image")]
pub use rskit_media_image as media_image;

/// Pure Rust audio analysis backend.
#[cfg(feature = "media-audio")]
pub use rskit_media_audio as media_audio;

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
