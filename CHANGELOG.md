# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Tooling

- Refactor CI and local validation around shared package-selection helpers, reduce PR test duplication by separating pinned behavioral tests from Ubuntu-only MSRV compile checks, and run feature-gated tests with explicit default/all-feature coverage.
- Rework coverage collection to run once per selected workspace group, preserve cached instrumented builds with profile-only cleanup, exclude the facade package from default coverage, honor explicit threshold overrides, and derive per-package summaries from workspace reports.
- Run push/full CI coverage with full cleanup so cached instrumented build artifacts cannot produce stale per-package line counts on `main`.
- Add focused authorization, configuration, lifecycle, DI, DAG, logging, messaging, resilience, Ollama, image, Redis, S3, Kafka, NATS, RabbitMQ, FFmpeg, and GCS adapter tests to raise package coverage with deterministic behavioral checks.
- Cover every `rskit-storage-s3` `FileStore` operation with wire-level request-construction and remote-failure tests using an in-process mock HTTP client, with no network or credentials.
- Add behavioral failure-path tests for the AI modules: MCP privileged tool-call denial, oversized-result, authorizer-error, and tool-failure auditing; agent limit precedence, budget mapping, and lifecycle; skill manifest path-traversal, asset-directory, and loader-config validation; and tool input fail-closed validation.
- Run doctests in CI and scope clippy to changed crates on pull requests for faster, more complete validation, while keeping full-scope checks on push and merge queues.
- Configure Dependabot to update the split `core/`, `contrib/`, and `examples/` Cargo workspaces, and extend CodeQL to scan Rust (build-mode none) alongside the existing GitHub Actions analysis.
- Redesign dependency-graph generation into a truthful domain-layer diagram plus an adapter-to-core graph, add a `make depgraphs` regeneration target, and cover the domain-graph reduction/rendering logic with focused tooling tests.

### Documentation

- Embed the regenerated domain-layer and contrib adapter dependency graphs in `docs/DESIGN.md`, document how to regenerate them, and remove the stale, out-of-sync graph copies.

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
