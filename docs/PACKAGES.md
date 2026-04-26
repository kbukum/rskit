# rskit Crate Map

rskit is a **Cargo workspace** with a facade crate (`rskit`) and 40+ independent sub-crates under `crates/`. Every crate has its own `README.md` — start there for API details. This file is the bird's-eye index.

## MSRV

**1.85** — enforced by `rust-toolchain.toml` and CI.

## Core

| Crate | Description |
|-------|-------------|
| `rskit` | Facade — re-exports all rskit-* crates |
| `rskit-errors` | Structured application error types with HTTP/gRPC status mapping |
| `rskit-config` | TOML + env var configuration loading with validation |
| `rskit-logging` | Structured logging with `tracing` — JSON in prod, pretty in dev |
| `rskit-bootstrap` | Typestate `App<S, C>`, Component registry, hooks |
| `rskit-provider` | Provider traits (request-response, stream, sink, duplex) |
| `rskit-pipeline` | Async data pipelines via `futures::Stream` extension operators |
| `rskit-resilience` | Retry, circuit breaker, bulkhead, rate limiter, tower layers |
| `rskit-worker` | Worker pool with `JoinSet`, typed events |
| `rskit-server` | tonic gRPC server bootstrap as a lifecycle-managed Component |

## Foundation

| Crate | Description |
|-------|-------------|
| `rskit-validation` | Fluent field-level validator with AppError conversion |
| `rskit-encryption` | AES-256-GCM and ChaCha20-Poly1305 |
| `rskit-http` | Axum HTTP server with graceful shutdown, CORS, request-ID |
| `rskit-di` | Lightweight Arc-based runtime DI container |
| `rskit-auth` | JWT, OIDC, password hashing, request-context auth helpers |

## Adapters

| Crate | Description |
|-------|-------------|
| `rskit-database` | sqlx async pool with repository pattern |
| `rskit-cache` | Redis client with typed store and Component lifecycle |
| `rskit-messaging` | Message broker abstractions with Kafka support |
| `rskit-httpclient` | Async HTTP client with auth and resilience |
| `rskit-grpc-client` | Tonic-based gRPC client with lazy connections |

## Platform

| Crate | Description |
|-------|-------------|
| `rskit-observability` | OpenTelemetry tracing, metrics, context propagation |
| `rskit-authz` | RBAC and ABAC authorization engine |
| `rskit-discovery` | Service discovery with load balancing strategies |
| `rskit-process` | Subprocess execution with process-group isolation |

## AI / ML

| Crate | Description |
|-------|-------------|
| `rskit-llm` | LLM provider abstractions (OpenAI, Anthropic) |
| `rskit-llm-providers` | LLM implementations — OpenAI, Anthropic, Gemini |
| `rskit-embedding` | Embedding provider abstractions for vector search |
| `rskit-inference` | Inference provider abstractions |
| `rskit-vector-store` | Vector store abstractions (Qdrant + in-memory) |
| `rskit-agent` | Agentic loop — LLM orchestration, tool execution |
| `rskit-tool` | Tool definitions, auto-wiring, registry, middleware |
| `rskit-hook` | Generic event hook system |
| `rskit-mcp` | Model Context Protocol server and client bridge |
| `rskit-schema` | JSON Schema generation and validation from Rust types |
| `rskit-explain` | Structured explanation generation via LLM |

## Media & File

| Crate | Description |
|-------|-------------|
| `rskit-file` | File I/O, storage backends, temp files, MIME detection |
| `rskit-file-s3` | S3 / S3-compatible (MinIO, LocalStack) backend |
| `rskit-media` | Media types, codec/format registry, pipeline builder |
| `rskit-media-ffmpeg` | FFmpeg CLI backend for video/audio |
| `rskit-media-image` | Native image processing (`image` crate) |
| `rskit-media-audio` | Pure Rust audio — WAV reading, waveform, silence detection |

## Specialist

| Crate | Description |
|-------|-------------|
| `rskit-testutil` | Test utilities, mock providers, assertion helpers |
| `rskit-sse` | Server-Sent Events bus with axum integration |
| `rskit-dag` | DAG task orchestrator with parallel execution |

## CLI & Data

| Crate | Description |
|-------|-------------|
| `rskit-cli` | CLI framework: progress bars, structured output, signals |
| `rskit-dataset` | Dataset collection: source, transform, target, collector |
| `rskit-bench` | ML benchmarking: evaluators, metrics, reports, visualization |

## Dependency Graph (core)

```
rskit-errors
rskit-config       → rskit-errors
rskit-logging      → rskit-config
rskit-resilience   → rskit-errors
rskit-provider     → rskit-errors, rskit-resilience
rskit-pipeline     → rskit-errors
rskit-bootstrap    → rskit-errors, rskit-config, rskit-logging
rskit-worker       → rskit-errors, rskit-provider, rskit-pipeline, rskit-resilience
rskit-server       → rskit-bootstrap, rskit-errors, rskit-config, rskit-resilience
rskit (facade)     → all above
```

No circular dependencies. `rskit-bootstrap` intentionally does **not** depend on `rskit-provider` or `rskit-worker` — components are registered as `Arc<dyn Component>`, keeping the core lifecycle thin.

See [`docs/adr/0001-layered-crate-architecture.md`](adr/0001-layered-crate-architecture.md) for the layering rationale.
