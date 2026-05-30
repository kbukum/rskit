# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed — Cross-Cutting
- **L9 infrastructure**: made the `rskit` facade a pure re-export layer,
  aligned facade feature flags with available crates, routed CLI-backed Git
  commands through `rskit-process`, standardized CLI error/output rendering,
  and added concrete testutil and benchmark helpers.
- **Module boundaries**: folded workload scheduling into `rskit-worker`,
  moved cross-layer integration coverage into the `rskit` facade tests,
  removed gRPC server re-exports from `rskit-grpc`, and feature-gated
  lifecycle-specific server/discovery surfaces.
- **Architecture guardrails**: added a topology check for removed wrapper
  crates, facade/workload cleanup, optional server transport stacks across
  dependency tables, and contrib-adapter aggregation boundaries.
- **Shared primitives**: added retry presets, checked HTTP response helpers,
  and storage metadata/key helpers with safe local path resolution to reduce
  repeated adapter boilerplate.
- **Maintainability refactor**: split large skill, MCP, agent, lifecycle, and
  AI-vocabulary modules around ownership boundaries; centralized Consul and LLM
  outbound HTTP/retry/telemetry mechanics on shared infrastructure. As part of
  the Consul migration, `ConsulDiscovery::new` now returns `AppResult<Self>` so
  HTTP client construction failures are surfaced at initialization. The
  `rskit-agent` stream API now drives the real turn loop so lifecycle events are
  emitted during execution with per-turn usage instead of replayed from the final
  run result.
- **Media/dataset**: redesigned dataset items around bounded byte/file
  payloads, streaming source contracts, explicit transform errors, schema
  validation via `rskit-schema`, and `rskit-pipeline` stream adapters.
- **rskit-process**: changed command program/argument storage to OS-native
  path/string values, added raw stdout/stderr bytes and truncation indicators
  to process results, and added line-observed process execution for streaming
  subprocess diagnostics.
- **L7 AI contracts**: strengthened tool schemas/input/output with typed wrappers,
  moved TGI/vLLM inference adapters from core to contrib, removed `reqwest::Error`
  from the public inference error surface, and aligned GenAI embedding span
  operation naming with current OpenTelemetry conventions.
- **Contrib adapters**: standardized storage, messaging, LLM, inference, and media adapters around explicit `Config` plus `register(&mut Registry, Config)` entry points with hidden vendor/concrete implementation surfaces.
- **L6 data backends**: aligned database, cache, storage, messaging, and vectorstore around explicit registries, config-key backend selection, and core-only in-memory/local defaults; moved Redis and Qdrant integrations to contrib adapters.
- **rskit-cache**: added a built-in filesystem cache adapter with a facade
  feature for filesystem-backed cache usage.
- **L6 auth/authz**: made request authentication fail closed by default with typed optional-auth outcomes, masked credential-bearing formatting paths, aligned OIDC HTTP usage with the canonical HTTP client, and added Tower authorization middleware.
- **L5 transport**: aligned HTTP/server/client/gRPC/SSE/discovery boundaries with explicit security ownership, direct HTTPS serving, baseline server middleware, toolkit-native SSE events, and explicit discovery registries.
- **rskit-observability**: replaced process-global tracer initialization with injectable OpenTelemetry providers for traces, metrics, and logs, with OTLP gRPC/HTTP support.
- **rskit-resilience**: added elapsed-time retry bounds and a composable Tower timeout layer.
- **rskit-http**: owns HTTP-specific CORS and response-header security policy; `rskit-server` consumes these HTTP transport capabilities.
- **rskit-security**: narrowed to cross-transport TLS/security configuration instead of HTTP-only behavior.

### Changed — Pattern Contracts
- **rskit-hook**: replaced public downcast-based hook payload handling with typed hook registration and added a bounded in-process event bus.
- **rskit-provider**: narrowed L2 provider contracts to canonical shapes plus `TowerProvider`, removing cross-cutting middleware ownership from the pattern crate.
- **rskit-component**: enforced registry start/stop timeout configuration in lifecycle state transitions.
- **rskit-di**: added typed `Resolve<T>` and `MustResolve<T>` resolver traits.
- **rskit-skill**: replaced the unmaintained YAML parser dependency with a maintained serde-compatible fork.
- **L4 composition crates**: aligned bootstrap lifecycle hooks with start/stop boundaries and typed lifecycle events, made pipeline fan-out/windowing bounded, replaced JSON chain operations with typed sequential composition, tightened DAG cycle/parallelism guarantees, removed worker ticker coupling, added typed state machines, and made process execution explicitly cancellable with bounded output by default.

### Changed — Foundations
- **rskit-process/config/validation/git**: added persistent process lifecycle
  primitives, typed config-template helpers, shared path-safe validation, and
  repository-relative path normalization utilities for reusable application
  infrastructure. `ProcessResult` now carries cancellation metadata and is
  non-exhaustive; construct results through `ProcessResult::completed`.
- **rskit-git**: added index entry reading so consumers can inspect staged file
  identities through the repository read API.
- **rskit-testutil**: added a managed `TestWorkspace` and `test_workspace!`
  macro for fixture-backed temporary test workspaces with safe path handling.
- **rskit-fs**: added a foundation crate for local filesystem primitives covering
  safe paths, file/directory/tree operations, temporary resources, links,
  permissions, and security-oriented defaults for reusable filesystem access.
- **rskit-util**: reduced the L0 utility crate to a minimal domain-free
  dependency surface.
- **rskit-config**: moved `SecretString` ownership out of `rskit-util`.
- **rskit-config**: made config loading precedence explicit with programmatic
  defaults and overrides; dotenv files now feed typed config loading without
  mutating the process environment.
- **rskit-errors**: removed mutable global problem-detail URI configuration and
  standardized cancellation HTTP mapping on `408 Request Timeout`.
- **rskit-version**: added canonical package-version helpers and routed service
  defaults/health metadata through them.

### Changed — Storage Adapter Boundaries
- **rskit-storage**: removed the feature-gated GCS backend from the core crate
  so storage remains local/trait-focused and does not own Google Cloud SDK
  dependencies.
- **rskit-storage-gcs**: added a dedicated Google Cloud Storage adapter crate
  implementing `rskit_storage::store::FileStore`, with application-default
  credentials by default and explicit anonymous mode for public buckets.
- **rskit**: `storage-gcs` now enables the `rskit-storage-gcs` adapter crate
  instead of a feature inside `rskit-storage`.

### Added — Documentation & Project Hygiene
- `SECURITY.md` — vulnerability disclosure policy, supply-chain section
  (cosign, CycloneDX SBOM, `cargo-audit`, `cargo-deny`).
- `GOVERNANCE.md` — roles, decision making, sibling-parity contract.
- `MAINTAINERS.md` — current maintainers, areas, succession.
- `docs/RELEASING.md` — mechanical release process for the cargo workspace
  (cargo-release, Trusted Publishing flow).
- `docs/VERSIONING.md` — workspace versioning guide (workspace inheritance).
- `docs/policy/SEMVER.md` — semantic-versioning policy aligned with Cargo's
  SemVer compatibility rules.
- `docs/policy/DEPRECATION.md` — deprecation lifecycle (`#[deprecated]`).
- `docs/adr/0000-template.md` and `docs/adr/0001-layered-crate-architecture.md` —
  Architecture Decision Records.
- `.github/CODEOWNERS` — review ownership across all crates.
- `.github/dependabot.yml` — cargo + GitHub Actions dependency automation.
- `.github/ISSUE_TEMPLATE/{bug_report,feature_request,config}.yml` — modern
  YAML form templates (replaces legacy `.md` templates).
- `.github/PULL_REQUEST_TEMPLATE.md` — expanded PR checklist with
  sibling-parity prompt and supply-chain checks.
- README: sibling-projects callout and `Project Documentation` index.

### Changed — Documentation Layout
- Moved `MEDIA_IMPLEMENTATION.md` (70 KB internal-only document) from the
  repo root to `core/rskit-media/docs/IMPLEMENTATION.md` to keep the
  top-level documentation surface focused on user-facing content.

### Added
- **rskit-messaging**: Event type with builder pattern and JSON serialization
- **rskit-messaging**: `MessageProducer` and `MessageConsumer` async traits for raw bytes
- **rskit-messaging**: `EventProducer` and `EventConsumer` async traits for typed events
- **rskit-messaging**: Kafka backend (feature-gated with `kafka` feature)
  - `KafkaProducer` implementing `MessageProducer` + `EventProducer`
  - `KafkaConsumer` implementing `MessageConsumer` + `EventConsumer`
- **rskit-messaging**: Extended `KafkaConfig` with security, SASL, retry fields
- **rskit-messaging**: `InMemoryBroker` Event support

#### Messaging Enhancement

- **rskit-messaging**: `ManagedProducer` — wraps any `MessageProducer` with lifecycle (start/stop), metrics collection, and running state
- **rskit-messaging**: `ManagedConsumer` — wraps any `MessageConsumer` with lifecycle, handler dispatch, and graceful shutdown
- **rskit-messaging**: `ConsumerRunner` — manages consumption loop as a tokio task with run/stop interface
- **rskit-messaging**: `MetricsCollector` trait with `record_publish()`/`record_consume()` and `NoopMetrics` impl
- **rskit-messaging**: `MessageTranslator` trait with `JsonTranslator` and `JsonStringTranslator` implementations
- **rskit-messaging**: `MessageHandler` trait + `FnHandler` adapter + `HandlerMiddleware` trait + `chain_handlers()` + `middleware_fn()`
- **rskit-messaging**: `MessageRouter` — topic routing with wildcard (`*`) pattern matching and default handler
- **rskit-messaging**: `BatchProducer` — buffered producer with size-based and time-based flush triggers
- **rskit-messaging**: Provider bridge (feature-gated `provider-bridge`):
  - `ProducerSink` — wraps `MessageProducer` as `Sink<Message<T>>`
  - `ConsumerStream` — wraps `MessageConsumer` as `StreamProvider<(), Message<T>>`
- **rskit-messaging**: Full middleware stack:
  - `RetryMiddleware` — exponential backoff with configurable max_attempts and backoff_factor
  - `DeadLetterMiddleware` — routes failed messages to `<topic>.dlq` via producer
  - `instrument()` — metrics middleware recording consume metrics via `MetricsCollector`
  - `tracing_middleware()` — OpenTelemetry spans with messaging.topic and messaging.key attributes
  - `DedupMiddleware` — deduplication with sliding window (size + TTL)
  - `CircuitBreakerMiddleware` — fail-fast using `rskit_resilience::CircuitBreaker`
- **rskit-messaging**: Enhanced `InMemoryBroker` with message history, topic tracking, and reset
- **rskit-messaging**: Test assertions — `assert_published()`, `assert_published_n()`, `assert_no_messages()`, `wait_for_message()`

## [0.1.0] - 2026-04-26

### Added

- Initial release of rskit workspace (49 crates)
- Core async runtime utilities (`rskit`)
- HTTP server with axum (`rskit-http`)
- Authentication: JWT, API keys, password hashing (`rskit-auth`)
- Configuration management (`rskit-config`)
- Structured logging with OpenTelemetry (`rskit-logging`, `rskit-observability`)
- Service mesh: discovery, gRPC, messaging (`rskit-discovery`, `rskit-messaging`)
- Database, cache, MQ adapters (`rskit-database`, `rskit-cache`, `rskit-messaging`)
- LLM provider integrations (`rskit-llm`, `rskit-llm-providers`)
- Media processing (`rskit-media`, `rskit-media-image`, `rskit-media-ffmpeg`)
- CLI tooling (`rskit-cli`)
- Comprehensive test utilities (`rskit-testutil`)

#### Detailed crate additions

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
- `App<Built, C>` typestate with `AppBuilder`, lifecycle hooks (`before_start`, `after_start`, `before_stop`, `after_stop`)
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
