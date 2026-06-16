# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Changed

- Adopt independent per-crate versioning: each crate now carries its own `version` instead of inheriting one lock-step workspace version, so releases republish only the crates that changed plus the correct caret cascade. Add a `release bump` command (`make release-bump W=<workspace>`) that detects changed crates since the last tag, applies patch-by-default / `MINOR` breaking bumps, cascades breaking minors to in-workspace dependents, rewrites caret floors, and is idempotent against the crates.io max published version.
- Release publisher waits out crates.io rate limits in short, deadline-driven slices so long pauses show a live countdown and self-correct across host suspend or clock changes.

### Fixed

- `release bump` now republishes crates whose only change is an inherited `[workspace.dependencies]` caret floor (e.g. a `core` breaking-minor that rewrites `contrib/Cargo.toml`), instead of reporting "No crates changed" and leaving the new floor unpublished, and diffs change detection from the tag merge-base so release branches and backports resolve the correct change set.
- Eliminate CodeQL `rust/hard-coded-cryptographic-value` alerts by generating AES-GCM/ChaCha20 salts and nonces directly from the RNG instead of zero-initialized buffers, and by deriving OIDC nonces and HTTP basic-auth credentials dynamically in tests.
- Order crates for publishing by their dev-dependencies as well as their normal and build dependencies, so a crate that dev-depends on an internal crate (e.g. `rskit-testutil`) is published after it; `cargo publish` requires every versioned dependency, including dev, to already exist on crates.io.

### Documentation

- Rework `docs/VERSIONING.md` and `docs/VERSIONING-ROADMAP.md` for the independent per-crate model (caret pins, 0.x semantics, per-workspace release trains), document the upstream-API watch-list and the future core/contrib repository sever, and update the `RELEASING.md` bump runbook to use the new tooling.

## [v0.1.0-alpha.1] - 2026-06-14

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
