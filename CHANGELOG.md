# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Tooling

- Rework coverage collection to run once per selected workspace group, preserve cached instrumented builds with profile-only cleanup, exclude the facade package from default coverage, honor explicit threshold overrides, and derive per-package summaries from workspace reports.
- Add focused authorization, configuration, lifecycle, DI, DAG, logging, messaging, resilience, Ollama, image, Redis, S3, Kafka, NATS, RabbitMQ, FFmpeg, and GCS adapter tests to raise package coverage with deterministic behavioral checks.
- Cover every `rskit-storage-s3` `FileStore` operation with wire-level request-construction and remote-failure tests using an in-process mock HTTP client, with no network or credentials.

### Fixed

- Sort JSON map keys in `rskit-media-image` and `rskit-media-ffmpeg` golden snapshot tests so they no longer depend on `serde_json`'s `preserve_order` feature, which workspace feature unification toggles depending on build scope and which made the snapshots fail intermittently.
- Install the rustls crypto provider before constructing Google Cloud Storage clients so GCS adapter tests and coverage runs do not panic under mixed TLS feature sets.

## [v0.1.0-alpha.1] - 2026-06-08

Initial alpha release of rskit, a Rust infrastructure toolkit for building services and reusable application foundations. This release establishes the first public baseline for the core, contrib, and example workspaces.

### Baseline capabilities

- Foundation crates for errors, configuration, logging, lifecycle management, dependency injection, validation, filesystem utilities, resilience policies, workers, pipelines, hooks, schemas, process execution, and graph/sequential composition.
- Service and transport foundations for HTTP, outbound HTTP clients, server composition, gRPC, SSE, discovery, authentication, authorization, and shared security policy.
- Data and integration foundations for databases, caches, storage, messaging, vector stores, datasets, Git integration, CLI helpers, benchmarking, and test utilities.
- AI and model infrastructure for LLM contracts, embeddings, inference, agents, tools, skills, MCP integration, and provider/runtime adapters.
- Media infrastructure for shared media abstractions, image/audio handling, and FFmpeg-backed processing.
- Contrib adapters for Redis, S3, GCS, Kafka, NATS, RabbitMQ, Qdrant, OpenAI, Anthropic, Gemini, Ollama, Triton, vLLM, TGI, FFmpeg, image, and audio integrations.

### Release model

- Establishes lock-step `0.1.0-alpha.1` versioning across publishable crates while the project is pre-stable.
- Publishes the facade package as `rskit-suite`; Rust code imports the facade crate as `rskit`.
- Uses split workspaces for `core/`, `contrib/`, and `examples/`, with examples validated but not published.
- Publishes crates in dependency order, keeping the facade package last.

### Quality and supply chain

- Adds release gates for formatting, linting, builds, tests, documentation, dependency policy, audit checks, coverage thresholds, publish dry-runs, SBOM generation, topology checks, and public API guardrails.
- Pins GitHub Actions by SHA and signs generated SBOM artifacts during release.
- Establishes security, contribution, governance, versioning, release, package discovery, and per-crate documentation.

### Stability notice

This is an **alpha preview** intended for early evaluation. APIs may change before the first stable release, especially while rskit is still aligning its foundational crates and adapter boundaries.

[Unreleased]: https://github.com/kbukum/rskit/compare/v0.1.0-alpha.1...HEAD
[v0.1.0-alpha.1]: https://github.com/kbukum/rskit/releases/tag/v0.1.0-alpha.1
