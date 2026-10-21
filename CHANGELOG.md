# Changelog

Notable changes, following [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) and [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- **encryption**: `new_key_material_encryptor` and `from_key_material` derive message keys from random key material (32 bytes or more, such as mounted key files) with HKDF-SHA256 in envelope version 2, so operations take microseconds instead of tens of milliseconds. Passphrase and key-material ciphertexts never open each other, and the format matches gokit through shared test vectors.
- **bench**: async semantic-similarity and LLM-judge metrics with bounded provider calls, validated results, and scoring provenance.
- **llm**: a canonical token-counting contract, heuristic default, and opt-in tiktoken/HuggingFace adapters.
- **bench**: token-usage metrics and explicit optimization direction for meaningful run comparisons.
- **testutil**: reusable HTTP request capture and deterministic embedding/LLM provider doubles.
- **errors**: preserve boxed error causes.

### Changed

- **Breaking (bench)**: `Metric::compute` and `Suite::compute` now return `AppResult`.
- **Breaking (bench)**: `RunProvenance` adds judge provenance and becomes non-exhaustive; construct from `Default` and use open patterns.
- **Breaking (process)**: `OutputObserver::with_stdout_bytes` and `with_stderr_bytes` take async sinks returning `AppResult<()>`; persistent byte callbacks also return `AppResult<()>`. Delivery applies backpressure and propagates failures, including during readiness. Incomplete observed output after the post-exit grace period returns a timeout error instead of silent output loss, for async and persistent processes.

### Fixed

- **bench**: semantic similarity rejects empty or dimension-inconsistent provider embeddings instead of scoring them as zero similarity.
- **worker**: pool shutdown reports accepted but unstarted work as cancelled, keeping it distinct from rejected submissions and failures in running tasks.
- **process**: PTY runs release the parent's terminal handles after spawn, so Linux output readers finish when the child exits instead of waiting out the grace period.

### Security

- Update `rustls` to 0.23.46, `quinn-proto` to 0.11.19 and `zip` to 9 to resolve RUSTSEC-2026-0346 to 0349 and the `quinn-proto` and `zip` advisories; `rskit-fs` archive extraction adapts to the zip 9 name API.
- Update `rustls-webpki` to 0.103.16, and `webpki-roots` and `webpki-root-certs` to 1.0.10, to resolve RUSTSEC-2026-0381, 0382 and 0383 (distrusted emSign intermediates).

## [v0.2.0-alpha.8] - 2026-08-16

### Added

- **util**: RFC 2822/5322 date-time parsing and formatting.
- **cli**: semantic action styling and configurable signal-driven shutdown.
- **process**: supervised child lifetimes across cancellation, shutdown, and scope exit.
- **git**: bounded revision-file reads and signed annotated tags with actionable signing errors.

### Changed

- Make Toven the canonical release and repository-gate driver while retaining rskit-specific policy checks.
- **Breaking (process)**: rename `SignalPolicy` to `LifecyclePolicy`.

### Fixed

- Propagate storage service failures instead of reporting false absence; bound Redis operations.
- Honor each supervised child's own shutdown policy.

## [v0.2.0-alpha.7] - 2026-08-02

### Added

- **git**: opt-in vendored OpenSSL and deterministic `main` initialization with explicit overrides.
- **codec**: YAML support for configuration-shaped documents.
- **testutil**: reusable golden comparisons, normalization, and explicit blessing.
- Configure split-workspace Toven release/coverage previews.
- **util**: typed SHA-256 digests and bounded streaming hashing.
- **fs**: deterministic archives and bounded extraction rejecting traversal and links.

### Fixed

- Classify process spawn failures and Git push/authentication/identity errors accurately.
- Stabilize image-resize performance comparisons under concurrent test load.

## [v0.2.0-alpha.6] - 2026-07-19

### Changed

- Organize CLI, benchmark, agent, media, data, transport, and auth code by concern with cohesive parameter models.
- Add advisory crowded-module detection.
- Expand real TLS, shutdown, and failure-path server coverage.
- Keep documentation prose naturally wrapped; remove mechanical prose-format gates.
- Reduce optional schema-validation, logging, and image-transform dependency footprints.
- Retry transient release-registry lookups with bounded backoff.
- Reject invalid media chunking configuration.
- **Breaking (vectorstore)**: `upsert` accepts `Point`; `search` accepts `SearchQuery`.
- **Breaking (storage)**: `upload` accepts `UploadOptions`; remove `upload_with_progress`.
- **Breaking (auth)**: `Manager::issue_key` accepts `KeySpec`.
- **Breaking (server)**: remove no-op request-ID, tracing, body-limit, and timeout builders; those layers remain automatic.

## [v0.2.0-alpha.5] - 2026-07-18

### Added

- **workload**: config-selected external container/pod orchestration and lifecycle contracts.
- Opt-in SQLite database and Supabase storage adapters.
- **cli**: interactive/line/scripted prompts, semantic glyphs/colors, and status feedback.
- **util**: unique-name resolution, bounded glob matching, and nearest-name suggestions.
- **fs**: owned debounced change streams.
- **process**: Unix PTY output.
- **stream**: bounded debounce batching.
- **errors**: cause-preserving advisory hints.

### Changed

- **Breaking (dataset)**: unify collection under `Collector<T>` with injected sinks and validators.
- **Breaking (worker)**: rename workload vocabulary to task/scheduler vocabulary, including `SchedulerConfig`, `TaskSpec`, `TaskBatch`, and `Scheduler`.
- **Breaking (messaging)**: receives require a positive timeout and return typed timeout errors.
- **Breaking (cli)**: live output uses a bounded virtual terminal; `LiveConfig` takes `rows` and `cols` instead of `tail_lines` and `width`.
- Apply shared workspace lints to examples and organize CLI/stream/process code by concern.
- Report watcher rescan requirements and bound process output draining after child exit.

### Fixed

- Restore non-Unix process compilation.
- Clean up blocking-runner children and workers on early failures.
- Update cargo-deny invocation compatibility.

## [v0.2.0-alpha.1] - 2026-06-29

### Added

- Coordinated workspace minor/major bumps, including new crates.
- **stream**: canonical async operators, fan-out, sinks, and owned cancellable tasks.
- **util**: shared content/SHA-256 hashing and dynamic templates.
- **process**: persistent output observers.
- **testutil**: serialized, restoring working-directory guards.
- **config**: composite merge identities, unique-key checks, and document-driven includes.

### Changed

- **Breaking (process)**: `PersistentConfig` adds `output_observer`; struct literals must supply it.
- **Breaking (git)**: tag messages use `Option<&str>`; `None` means lightweight and `Some` means annotated.
- **Breaking (config)**: custom `MergeIdentity` implementations provide `label` and `identity_of` instead of `identity_key`.
- Restore downward config layering and reuse canonical hashing/templates.
- Extend inherited documentation and workspace lints.

### Removed

- **Breaking**: remove `rskit-pipeline`; stream APIs move to `rskit-stream`, and sequential execution belongs to `rskit-chain`.
- Remove `observability::init_tracer` in favor of `tracer_provider`, and legacy bench `report`/`metrics` modules in favor of `report_gen`/`metric`.
- Remove unused direct adapter HTTP dependencies.

### Fixed

- Restore benchmark coverage acceptance.

## [v0.1.0-alpha.3] - 2026-06-21

### Added

- Document consumer-neutral foundation contracts and add a core-only CLI example.

### Changed

- Give logging ownership of its configuration with an optional setup stack, restoring downward config layering.
- Remove unused dependencies across foundation, composition, transport, data, AI, and infrastructure crates.

### Fixed

- Restore codec coverage acceptance.

## [v0.1.0-alpha.2] - 2026-06-16

### Changed

- Adopt independent crate versions, dependency-floor cascades, and facade release-train bumps.
- Keep rate-limited publishing responsive to deadlines.

### Fixed

- Include inherited dependency-floor changes and correct backport baselines in releases.
- Remove hard-coded cryptographic values from generation and test paths.
- Publish in dependency order including development dependencies.

### Documentation

- Align versioning and release guides with independent crate versions and future workspace boundaries.

## [v0.1.0-alpha.1] - 2026-06-14

Initial alpha baseline for reusable Rust infrastructure.

### Baseline capabilities

- Foundation: errors, config, logging, lifecycle, DI, validation, filesystem, resilience, workers, streams, schemas, process execution, and composition.
- Services: HTTP/gRPC/SSE, discovery, authentication, authorization, and security policy.
- Data: databases, caches, storage, messaging, vectors, datasets, Git, CLI, benchmarks, and testing.
- AI: LLMs, embeddings, inference, agents, tools, skills, MCP, and provider adapters.
- Media: shared vocabulary, image/audio handling, and FFmpeg processing.
- Contrib: Redis, cloud storage, messaging, vector, model, inference, and media integrations.

### Release model

- Lock-step alpha versions across publishable crates; `rskit-suite` is imported as `rskit`.
- Separate core, contrib, and example workspaces; examples are validated but not published.
- Dependency-ordered publishing with the facade last.

### Quality and supply chain

- Build, test, lint, docs, coverage, dependency, topology, public-API, and publication gates.
- SHA-pinned actions, signed SBOM artifacts, and contribution/security/release policies.

### Stability notice

Alpha APIs may change before the stable release.

[Unreleased]: https://github.com/kbukum/rskit/compare/v0.2.0-alpha.8...HEAD
[v0.2.0-alpha.8]: https://github.com/kbukum/rskit/compare/v0.2.0-alpha.7...v0.2.0-alpha.8
[v0.2.0-alpha.7]: https://github.com/kbukum/rskit/compare/v0.2.0-alpha.6...v0.2.0-alpha.7
[v0.2.0-alpha.6]: https://github.com/kbukum/rskit/compare/v0.2.0-alpha.5...v0.2.0-alpha.6
[v0.2.0-alpha.5]: https://github.com/kbukum/rskit/compare/v0.2.0-alpha.4...v0.2.0-alpha.5
[v0.2.0-alpha.1]: https://github.com/kbukum/rskit/compare/v0.1.0-alpha.3...v0.2.0-alpha.1
[v0.1.0-alpha.3]: https://github.com/kbukum/rskit/compare/v0.1.0-alpha.2...v0.1.0-alpha.3
[v0.1.0-alpha.2]: https://github.com/kbukum/rskit/compare/v0.1.0-alpha.1...v0.1.0-alpha.2
[v0.1.0-alpha.1]: https://github.com/kbukum/rskit/releases/tag/v0.1.0-alpha.1
