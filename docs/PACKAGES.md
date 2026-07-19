# rskit Package Catalog

rskit is distributed as focused crates instead of one monolithic package.
Use the `rskit-suite` facade package when you want common modules behind feature flags,
or depend directly on individual crates when you want a narrower dependency graph.

The repository has three workspace manifests and intentionally no root `Cargo.toml`:

| Workspace | Purpose | Manifest |
|---|---|---|
| Core | Foundation crates and the `rskit-suite` facade | [`core/Cargo.toml`](../core/Cargo.toml) |
| Contrib | Vendor and infrastructure adapters | [`contrib/Cargo.toml`](../contrib/Cargo.toml) |
| Examples | Demo applications validated by CI, not published | [`examples/Cargo.toml`](../examples/Cargo.toml) |

All publishable crates currently use the same pre-1.0 version from their owning workspace.
See [Versioning](VERSIONING.md) for compatibility policy
and [Releasing](RELEASING.md) for the mechanical release runbook.

## How to choose crates

| If you need | Start with |
|---|---|
| A simple application dependency with opt-in features | [`rskit-suite`](../core/rskit/README.md) |
| Stable error/config/logging foundations | `rskit-errors`, `rskit-config`, `rskit-logging` |
| Lifecycle-managed services | `rskit-bootstrap`, `rskit-component`, `rskit-server` |
| HTTP/gRPC/SSE transport | `rskit-http`, `rskit-httpclient`, `rskit-grpc`, `rskit-sse` |
| Fault tolerance | `rskit-resilience` |
| Async streams and workers | `rskit-stream`, `rskit-worker`, `rskit-dag` |
| Data abstractions | `rskit-database`, `rskit-cache`, `rskit-storage`, `rskit-messaging` |
| AI/model infrastructure | `rskit-ai`, `rskit-llm`, `rskit-embedding`, `rskit-inference`, `rskit-vectorstore`, `rskit-agent`, `rskit-tool`, `rskit-mcp` |
| Media processing | `rskit-media` plus `rskit-media-*` adapters |
| Test support | `rskit-testutil` as a dev-dependency |

## Core workspace

| Crate | Role |
|---|---|
| [`rskit-suite`](../core/rskit/README.md) | Facade package that re-exports foundation crates as the Rust crate `rskit` and exposes adapter integrations through feature flags. |
| [`rskit-util`](../core/rskit-util/README.md) | Domain-free utility primitives for secrets, templates, strings, collections, environment parsing, byte sizes, duration parsing, UTC formatting, and backoff helpers. |
| [`rskit-fs`](../core/rskit-fs/README.md) | Local filesystem primitives for confined paths, files, directories, temp files, links, permissions, and atomic writes. |
| [`rskit-version`](../core/rskit-version/README.md) | Build-time version and git metadata helpers. |
| [`rskit-errors`](../core/rskit-errors/README.md) | Structured application errors, error codes, RFC 9457 problem details, and HTTP status metadata. |
| [`rskit-config`](../core/rskit-config/README.md) | Layered configuration loading, validation, dotenv/env precedence, and redacted secret fields. |
| [`rskit-logging`](../core/rskit-logging/README.md) | Structured logging setup on `tracing`, including JSON/pretty output and optional OTLP integration. |
| [`rskit-component`](../core/rskit-component/README.md) | Component lifecycle primitives: `Component`, `Registry`, health, and state. |
| [`rskit-bootstrap`](../core/rskit-bootstrap/README.md) | Typestate application lifecycle orchestration with component registry and hooks. |
| [`rskit-provider`](../core/rskit-provider/README.md) | Request/response, stream, sink, duplex provider traits and a Tower bridge. |
| [`rskit-stream`](../core/rskit-stream/README.md) | Foundational async stream toolkit: bounded fan-out broadcaster, sources, cancellable tasks, and `futures::Stream` extension operators. |
| [`rskit-resilience`](../core/rskit-resilience/README.md) | Retry, circuit breaker, bulkhead, rate limiter, timeout, and Tower layers. |
| [`rskit-process`](../core/rskit-process/README.md) | Subprocess execution with cancellation, timeout, process-group isolation, and bounded output. |
| [`rskit-worker`](../core/rskit-worker/README.md) | Task worker pools with `JoinSet`, typed events, provider bridges, and backpressure. |
| [`rskit-chain`](../core/rskit-chain/README.md) | Sequential chain execution pattern for typed operations. |
| [`rskit-stateful`](../core/rskit-stateful/README.md) | Stateful accumulators with triggers, measurers, and TTL cleanup. |
| [`rskit-server`](../core/rskit-server/README.md) | Lifecycle-managed service-facing server abstractions and HTTP/gRPC transport composition. |
| [`rskit-validation`](../core/rskit-validation/README.md) | Fluent field-level validation that collects errors and converts to `AppError`. |
| [`rskit-encryption`](../core/rskit-encryption/README.md) | Encryption helpers for AES-256-GCM and ChaCha20-Poly1305. |
| [`rskit-http`](../core/rskit-http/README.md) | Framework-neutral HTTP abstractions, policies, and Tower adapters. |
| [`rskit-httpclient`](../core/rskit-httpclient/README.md) | Async outbound HTTP client with auth, destination policy, response limits, and injected resilience. |
| [`rskit-di`](../core/rskit-di/README.md) | Lightweight `Arc`-based runtime dependency injection container. |
| [`rskit-auth`](../core/rskit-auth/README.md) | JWT, OIDC, password hashing, and request-context authentication helpers. |
| [`rskit-authz`](../core/rskit-authz/README.md) | RBAC and ABAC authorization engine. |
| [`rskit-security`](../core/rskit-security/README.md) | Shared TLS, authentication scheme constants, and security configuration for transports. |
| [`rskit-database`](../core/rskit-database/README.md) | Database contracts with in-memory defaults and opt-in adapter backends. |
| [`rskit-cache`](../core/rskit-cache/README.md) | Cache abstraction with explicit store registration and local adapters. |
| [`rskit-messaging`](../core/rskit-messaging/README.md) | Message broker contracts, registry, middleware, and in-memory adapter. |
| [`rskit-storage`](../core/rskit-storage/README.md) | File I/O, local storage, temp files, MIME detection, and storage backend traits. |
| [`rskit-vectorstore`](../core/rskit-vectorstore/README.md) | Vector store abstraction with typed payloads, limits, in-memory default, and adapter registry. |
| [`rskit-observability`](../core/rskit-observability/README.md) | OpenTelemetry traces, metrics, logs, GenAI attributes, and context propagation. |
| [`rskit-discovery`](../core/rskit-discovery/README.md) | Service discovery with load-balancing strategies and transport integration points. |
| [`rskit-git`](../core/rskit-git/README.md) | Composable git repository interfaces backed by libgit2 and CLI implementations. |
| [`rskit-grpc`](../core/rskit-grpc/README.md) | Tonic gRPC transport support and `AppError` status mapping. |
| [`rskit-sse`](../core/rskit-sse/README.md) | Server-Sent Events bus with Axum integration. |
| [`rskit-dag`](../core/rskit-dag/README.md) | DAG task orchestration with cycle detection and bounded parallel execution. |
| [`rskit-ai`](../core/rskit-ai/README.md) | Shared AI/model vocabulary for LLM, embedding, inference, and observability crates. |
| [`rskit-llm`](../core/rskit-llm/README.md) | SDK-free chat completion contracts, canonical tool-use blocks, stream events, and provider trait. |
| [`rskit-embedding`](../core/rskit-embedding/README.md) | SDK-free embedding contracts and deterministic in-memory provider for tests. |
| [`rskit-inference`](../core/rskit-inference/README.md) | Model-serving runtime inference abstractions and registry. |
| [`rskit-schema`](../core/rskit-schema/README.md) | JSON Schema generation and bounded schema validation from Rust types. |
| [`rskit-tool`](../core/rskit-tool/README.md) | Tool definitions, auto-wiring, registries, and middleware for agentic systems. |
| [`rskit-skill`](../core/rskit-skill/README.md) | SDK-free skill manifests, loaders, registries, and verification contracts. |
| [`rskit-hook`](../core/rskit-hook/README.md) | Generic typed event hooks and in-process dispatch. |
| [`rskit-agent`](../core/rskit-agent/README.md) | Turn-based agent loop over providers, tools, hooks, and usage events. |
| [`rskit-media`](../core/rskit-media/README.md) | Media types, codec/format registry, pipeline builder, and processing traits. |
| [`rskit-mcp`](../core/rskit-mcp/README.md) | Bridge between the rskit tool registry and Model Context Protocol. |
| [`rskit-cli`](../core/rskit-cli/README.md) | CLI framework helpers for progress bars, structured output, errors, and signals. |
| [`rskit-dataset`](../core/rskit-dataset/README.md) | Dataset collection framework with source, transform, target, collector, and bounded payloads. |
| [`rskit-bench`](../core/rskit-bench/README.md) | ML benchmarking framework with evaluators, metrics, reports, visualization, and storage. |
| [`rskit-testutil`](../core/rskit-testutil/README.md) | Test utilities, mock providers, assertions, and managed test workspaces. |

## Contrib workspace

| Crate | Role |
|---|---|
| [`rskit-cache-redis`](../contrib/cache/redis/README.md) | Redis adapter for `rskit-cache`. |
| [`rskit-storage-s3`](../contrib/storage/s3/README.md) | Amazon S3 and S3-compatible storage backend for `rskit-storage`. |
| [`rskit-storage-gcs`](../contrib/storage/gcs/README.md) | Google Cloud Storage backend for `rskit-storage`. |
| [`rskit-messaging-kafka`](../contrib/messaging/kafka/README.md) | Kafka adapter for `rskit-messaging`. |
| [`rskit-messaging-nats`](../contrib/messaging/nats/README.md) | NATS adapter for `rskit-messaging`. |
| [`rskit-messaging-rabbitmq`](../contrib/messaging/rabbitmq/README.md) | RabbitMQ adapter for `rskit-messaging`. |
| [`rskit-vectorstore-qdrant`](../contrib/vectorstore/qdrant/README.md) | Qdrant adapter for `rskit-vectorstore`. |
| [`rskit-inference-tgi`](../contrib/inference/tgi/README.md) | Hugging Face TGI REST adapter for `rskit-inference`. |
| [`rskit-inference-triton`](../contrib/inference/triton/README.md) | Triton KServe v2 HTTP adapter for `rskit-inference`. |
| [`rskit-inference-vllm`](../contrib/inference/vllm/README.md) | vLLM REST adapter for `rskit-inference`. |
| [`rskit-llm-common`](../contrib/llm/common/README.md) | Shared LLM provider parsing and error utilities used by contrib LLM adapters. |
| [`rskit-llm-openai`](../contrib/llm/openai/README.md) | OpenAI provider implementation for `rskit-llm`. |
| [`rskit-llm-anthropic`](../contrib/llm/anthropic/README.md) | Anthropic provider implementation for `rskit-llm`. |
| [`rskit-llm-gemini`](../contrib/llm/gemini/README.md) | Gemini provider implementation for `rskit-llm`. |
| [`rskit-llm-ollama`](../contrib/llm/ollama/README.md) | Ollama provider implementation for `rskit-llm`. |
| [`rskit-media-audio`](../contrib/media/audio/README.md) | Pure Rust audio backend for WAV I/O, waveform, silence detection, and loudness. |
| [`rskit-media-ffmpeg`](../contrib/media/ffmpeg/README.md) | FFmpeg CLI backend for video and audio processing. |
| [`rskit-media-image`](../contrib/media/image/README.md) | Native image processing backend built on the `image` crate. |

## Local development commands

Use the Makefile for repo-wide work so each split workspace is addressed correctly:

```sh
make build
make test
make lint
make doc
make deny
```

For a single workspace, pass `W=core`, `W=contrib`, or `W=examples`. For one crate,
pass `C=<crate-name>`:

```sh
make test W=core
make lint C=rskit-errors
make doc C=rskit-storage-s3
```

If you call Cargo directly, always pass the owning manifest:

```sh
cargo test --manifest-path core/Cargo.toml -p rskit-errors
cargo test --manifest-path contrib/Cargo.toml -p rskit-storage-s3
cargo test --manifest-path examples/Cargo.toml --workspace
```

## Dependency direction

Lower-level crates must not depend on higher-level crates.
The canonical dependency policy is ADR [0001: Layered crate architecture](adr/0001-layered-crate-architecture.md),
enforced by `make check-topology` and `make deny`.
