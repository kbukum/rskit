# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

_No unreleased changes._

## [v0.1.0-alpha.1] - 2026-06-08

### Added

- Initial alpha release of the rskit workspace: foundational Rust infrastructure crates for service development.
- Core foundation crates for structured errors, layered configuration, logging, lifecycle orchestration, component registries, provider contracts, resilience policies, validation, filesystem utilities, workers, pipelines, hooks, schemas, version metadata, process execution, dependency injection, and sequential/DAG composition.
- Transport and service crates for framework-neutral HTTP abstractions, outbound HTTP clients, server composition, gRPC, SSE, discovery, shared security policy, authentication, and authorization.
- Data and integration crates for databases, caches, storage, messaging, vector stores, datasets, Git integration, CLI helpers, test utilities, and benchmarking.
- AI/model infrastructure crates for shared AI vocabulary, LLM contracts, embeddings, inference, tools, skills, agents, MCP integration, and provider/runtime adapters.
- Media crates for media abstractions, image processing, audio handling, and FFmpeg-backed processing.
- Contrib adapter crates for Redis, S3, GCS, Kafka, NATS, RabbitMQ, Qdrant, OpenAI, Anthropic, Gemini, Ollama, Triton, vLLM, TGI, FFmpeg, image, and audio integrations.
- Repository documentation for onboarding, package discovery, examples, integration patterns, versioning, release operations, security policy, governance, contribution workflow, ADRs, and per-crate usage.

### Release and distribution

- Crates are released in lock-step as `0.1.0-alpha.1` from split Cargo workspaces under `core/`, `contrib/`, and `examples/`.
- The facade package is published as `rskit-toolkit` because `rskit` is already owned on crates.io; Rust code imports the facade as `rskit`.
- Releases are published by creating a GitHub Release from the repository Releases UI. Publishing the GitHub Release triggers validation, crates.io publishing, SBOM generation, signing, and release asset upload.
- Publish tooling resolves crates in dependency order and keeps the facade package last so focused crates are available before the aggregate facade is published.

### Security and quality

- Release gates cover formatting, linting, builds, tests, documentation, dependency policy, audit checks, coverage thresholds, publish dry-runs, SBOM generation, topology checks, public API guardrails, and SHA-pinned GitHub Actions.
- Runtime-oriented crate designs use typed errors, bounded I/O, explicit configuration, root-confined filesystem helpers, redacted secret values, current TLS/crypto dependencies, and adapter registries with explicit registration.
- The first release is marked as an alpha preview for downstream evaluation before any stable compatibility commitment.

[Unreleased]: https://github.com/kbukum/rskit/compare/v0.1.0-alpha.1...HEAD
[v0.1.0-alpha.1]: https://github.com/kbukum/rskit/releases/tag/v0.1.0-alpha.1
