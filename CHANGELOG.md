# Changelog

All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Add `CurrentDirGuard` to `rskit-testutil`: an RAII guard that serializes process-wide working-directory changes across tests in a binary and restores the previous directory on drop (the working-directory analogue of the env-var mutex pattern), so tests that depend on or mutate the current directory no longer leak global cwd state into later tests.
- Extend `rskit-config` strict include-merge with richer identity rules and document-driven includes: generalize the `MergeIdentity` trait to a human-facing `label()` plus an `identity_of(&Value)` token extractor; add `CompositeKey` for multi-field and nested (dotted-path) identities encoded injectively so no field value can forge an identity boundary; add `IncludeMerge::with_unique_keys` to hard-error on a map key (`[section.<name>]`) contributed by more than one merged document instead of silently last-wins merging; and add `StrictLoader::load_resolving_includes` / `load_raw_resolving_includes` to read the canonical document once and derive its include list from inside it.

### Changed

- **Breaking (`rskit-config`):** `MergeIdentity` no longer exposes `identity_key()`. Custom implementors now provide `label()` (the error-message name) and `identity_of(&Value) -> Option<String>` (the identity token). The built-in `IdentityKey` is unchanged at call sites; only hand-rolled `MergeIdentity` implementations need to migrate.

## [v0.1.0-alpha.3] - 2026-06-21

### Added

- Document consumer classes (service / app / CLI / tool / library) and the guarantees the foundation layer makes to each in `docs/CONSUMER-CLASSES.md`, so new core crates are designed consumer-neutral from day one.
- Add the `core-cli` example: a real CLI built purely on `core/` crates (`rskit-config`, `rskit-logging`, `rskit-cli`, `rskit-errors`, `rskit-version`) that intentionally links no transport/server crate. It serves as a standing regression guard against service-shaped creep in the foundation layer.

### Changed

- Decouple `rskit-logging` from `rskit-config`: `rskit-logging` now owns its configuration vocabulary (`LoggingConfig` / `LogFormat` / `LogOutput`) as tracing-free `serde` data in a new always-on `config` module, and `rskit-config` consumes it instead of defining it. `rskit-config` re-exports the types under its `validate` feature, so `rskit_config::{LoggingConfig, LogFormat, LogOutput}` stays stable. `rskit-logging` gains a default-on `setup` feature gating the `tracing`/`tracing-subscriber` subscriber stack (`init_logging*`, `LoggingGuard`, masking, sampling, per-module levels, context, OTLP), so consumers can depend on the vocabulary alone via `default-features = false`. `rskit-logging` no longer depends on `rskit-config`, restoring downward layering.
- Prune patterns-domain dependencies (no behavior change): `rskit-provider` drops unused `bytes`, `futures-util`, `pin-project-lite`, `tower-layer`, `tower-service`, and `tracing`; `rskit-hook` drops unused `tracing`; `rskit-bootstrap` drops unused `futures` and `rskit-logging` and relocates test-only `async-trait`, `validator`, `rskit-validation`, `serde`, and `parking_lot` to `dev-dependencies`.
- Prune crosscutting-domain dependencies (no behavior change): `rskit-observability` drops unused `tracing-subscriber`; `rskit-resilience` drops unused `futures-util`, `pin-project-lite`, `serde`, `tower-layer`, and `tower-service`; `rskit-auth` drops unused `rskit-config` and `uuid`.
- Prune composition-domain dependencies (no behavior change): `rskit-pipeline` drops unused `pin-project-lite` and `tracing`; `rskit-dag` drops unused `futures`; `rskit-worker` drops unused `futures-util`; `rskit-chain` drops unused `tracing`.
- Prune transport-domain dependencies (no behavior change): `rskit-server` drops `tonic-health`/`tower-layer`/`tower-service`; `rskit-httpclient` drops `thiserror`; `rskit-grpc` drops `chrono`/`tower`; `rskit-sse` drops `async-trait`; `rskit-messaging` drops `tokio-stream` and `rskit-bootstrap`.
- Prune data-domain dependencies (no behavior change): `rskit-database` drops `chrono`/`uuid`/`tracing`/`rskit-config`; `rskit-cache` drops `tracing`; `rskit-storage` drops `pin-project-lite`; `rskit-dataset` drops `rskit-fs`.
- Prune ai-domain dependencies (no behavior change): `rskit-llm` drops unused `tracing` (and redundant dev-dep `serde_json`); `rskit-inference` drops unused `uuid`; `rskit-skill` drops unused `serde_json`; `rskit-mcp` drops unused `thiserror` and `rskit-skill`; `rskit-media` drops unused `tracing` (and redundant dev-deps `serde_json`/`rskit-storage`).
- Prune infra-domain dependencies (no behavior change): `rskit-discovery` drops unused `tokio-stream`; `rskit-process` drops unused `thiserror`.

### Fixed

- Restore the full-coverage gate on `main`: `rskit-codec` now exercises the `encode`/`decode` free functions (round-trip plus conversion, type-mismatch, and parse-error paths) and previously-uncovered `TomlCodec`/value-merge error branches, raising line coverage above the 90% push-to-main threshold (no library behavior change).

## [v0.1.0-alpha.2] - 2026-06-16

### Changed

- Adopt independent per-crate versioning: each crate now carries its own `version` instead of inheriting one lock-step workspace version, so releases republish only the crates that changed plus the correct caret cascade. Add a `release bump` command (`make release-bump W=<workspace>`) that detects changed crates since the last tag, applies patch-by-default / `MINOR` breaking bumps, cascades breaking minors to in-workspace dependents, rewrites caret floors, and is idempotent against the crates.io max published version.
- Release publisher waits out crates.io rate limits in short, deadline-driven slices so long pauses show a live countdown and self-correct across host suspend or clock changes.
- `release bump` now treats the `rskit-suite` facade as the release-train umbrella: a crate marked `[package.metadata.release] umbrella = true` is force-bumped whenever any other crate in its workspace is bumped, so the facade version always reflects the headline release even when only re-exported crates changed (idempotent against the released baseline).

### Fixed

- `release bump` now republishes crates whose only change is an inherited `[workspace.dependencies]` caret floor (e.g. a `core` breaking-minor that rewrites `contrib/Cargo.toml`), instead of reporting "No crates changed" and leaving the new floor unpublished, and diffs change detection from the tag merge-base so release branches and backports resolve the correct change set. Transient crates.io lookup failures now degrade to tag-only baselines with a warning rather than aborting the bump.
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

[Unreleased]: https://github.com/kbukum/rskit/compare/v0.1.0-alpha.3...HEAD
[v0.1.0-alpha.3]: https://github.com/kbukum/rskit/compare/v0.1.0-alpha.2...v0.1.0-alpha.3
[v0.1.0-alpha.2]: https://github.com/kbukum/rskit/compare/v0.1.0-alpha.1...v0.1.0-alpha.2
[v0.1.0-alpha.1]: https://github.com/kbukum/rskit/releases/tag/v0.1.0-alpha.1
