# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added
- **rskit-messaging**: Event type with builder pattern and JSON serialization
- **rskit-messaging**: `EventProducer` and `EventConsumer` async traits
- **rskit-messaging**: Kafka backend (feature-gated with `kafka` feature)
  - `KafkaProducer` implementing `MessageProducer` + `EventProducer`
  - `KafkaConsumer` implementing `MessageConsumer` + `EventConsumer`
- **rskit-messaging**: Extended `KafkaConfig` with security, SASL, retry fields
- **rskit-messaging**: `InMemoryBroker` Event support

## [0.1.0] - 2024-01-01

### Added

#### `rskit-errors`
- `ErrorCode` enum with 17 variants covering auth, input, resource, and infrastructure errors
- `AppError` struct with fluent builder, optional chained `cause`, and key/value `details`
- `AppResult<T>` type alias
- `is_retryable()` and `http_status()` on `ErrorCode`
- Convenience constructors: `not_found`, `unauthorized`, `forbidden`, `conflict`, `invalid_input`, `timeout`, `rate_limited`, `service_unavailable`, `internal`, `database_error`, `external_service`
- `From<AppError>` for `tonic::Status` and `From<tonic::Status>` for `AppError`

#### `rskit-config`
- `ConfigLoader` with layered loading: TOML file → `.env` → environment variables (`APP__` prefix by default)
- `AppConfig` trait (requires `DeserializeOwned + Validate`)
- `ServiceConfig`, `LoggingConfig`, `Environment`, `LogFormat` built-in types
- `load_config<T>` convenience free function

#### `rskit-logging`
- `init_logging(cfg)` returning a `LoggingGuard` (dropped on shutdown)
- `init_logging_env()` for `RUST_LOG`-only setup
- JSON and console format support via `LogFormat`

#### `rskit-bootstrap`
- `Component` async trait with `start`, `stop`, `health`
- `Health`, `HealthStatus` (Healthy / Degraded / Unhealthy)
- `Registry` — ordered `start_all` / reverse-order `stop_all`
- `App<Unconfigured, C>` typestate with `AppBuilder`, lifecycle hooks (`on_configure`, `on_start`, `on_ready`, `on_stop`)
- `run_task` for driving a single async closure with graceful shutdown

#### `rskit-resilience`
- `RetryPolicy` — exponential backoff with jitter, configurable `retry_if` predicate
- `CircuitBreaker` — Closed / Open / HalfOpen state machine (non-poisoning `parking_lot::Mutex`)
- `Bulkhead` — semaphore-backed concurrency limit with timeout
- `RateLimiter` — `governor`-backed atomic token bucket with `check()` and `until_ready()`
- Tower layers: `RetryLayer`, `CircuitBreakerLayer`, `BulkheadLayer`, `RateLimitLayer`

#### `rskit-provider`
- `Provider`, `RequestResponse<I,O>`, `StreamProvider<I,O>`, `Sink<I>`, `Duplex<I,O>` traits
- `TowerProvider<S,I,O>` bridge from any `tower::Service`
- `request_response_fn` and `sink_fn` convenience constructors
- Middleware layers: `LoggingLayer`, `TracingLayer`, `ResilienceLayer`

#### `rskit-pipeline`
- `RskitStreamExt` extension trait on `futures::Stream` with 13 operators:
  `rmap`, `rflatmap`, `rfilter`, `rtap`, `rreduce`, `rparallel`, `rfan_out`, `rbatch`,
  `rdebounce`, `rthrottle`, `rtumbling_window`, `rsliding_window`
- Stream sources: `from_slice`, `from_fn`, `from_channel`, `merge`, `concat`
- Stream terminals: `collect`, `for_each`

#### `rskit-worker`
- `Handler<I,O>` async trait
- `Event<O>` with `EventKind` (Progress, Partial, Log, Result, Error) and `Progress` helper
- `TaskHandle<O>` with `result()`, `events()`, `cancel()`
- `Pool<I,O>` — `JoinSet` + `Semaphore` bounded pool, mpsc→broadcast event relay
- `PoolConfig` builder with `with_size`, `with_queue_size`, `with_grace_period`
- `from_provider` / `as_provider` bidirectional bridges with `rskit-provider`

#### `rskit-server`
- `GrpcServerConfig` with `validator` support and optional TLS
- `GrpcServerBuilder` with `add_service`, `with_reflection`, `with_health_check`
- `GrpcServer` implementing `rskit_bootstrap::Component`

#### `rskit` (facade)
- Re-exports all sub-crates under a single dependency

[Unreleased]: https://github.com/kbukum/rskit/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/kbukum/rskit/releases/tag/v0.1.0
