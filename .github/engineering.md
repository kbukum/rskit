# rskit engineering reference

Read only the section needed for the task. [Agent entry point](copilot-instructions.md).

Rust infrastructure toolkit providing foundational crates for service development.
Sibling kit to gokit (Go): aligned in capabilities and naming where
idiomatic, no kit the canonical reference.

## Engineering principles

Shared engineering baseline — apply to all work here:

- **Phases:** discover → decide (Redesign / Align / Enhance / Drop / Leave) → implement completely → validate. Prefer root-cause redesign over symptom patches; no compatibility shims in pre-stable code. Implement the *simplest* design that fully solves it — flexible, extensible, and scalable — on current idiomatic best practices, not folklore; complexity must earn its place.
- **Layering & reuse:** explicit, acyclic dependency direction — lower layers never import higher. Reuse or enhance the canonical owner before writing new code; never duplicate shared concerns (errors, config, logging, auth, retries, observability, HTTP, registries). Consult [`docs/CONCERN-OWNERS.md`](../docs/CONCERN-OWNERS.md) for the canonical owner of each shared concern (formats → `rskit-codec`, helpers → `rskit-util`, paths → `rskit-fs`, …) before writing new code.
- **APIs:** typed and minimal; no broad `Any` / `interface{}` / unchecked `unknown` in public surfaces; actionable typed errors that preserve cause.
- **Errors & resilience:** no panics / unwrap or swallowed errors on runtime paths; no success-shaped fallbacks; timeout every remote call; bounded jittered retries for idempotent ops only; circuit-break and degrade gracefully.
- **Concurrency:** every task has ownership, cancellation, timeout, and shutdown; bound queues / buffers / concurrency with documented backpressure; drain on shutdown.
- **Security & privacy:** validate at every trust boundary; least-privilege and secure-by-default; parameterized queries and argv-only subprocess; tokens in headers, not query strings; current crypto only; minimize, redact, and retention-bound sensitive data.
- **Composition:** explicit injected registries and config-driven selection; no import-time side effects, no mutable global registries; inject logger / tracer / policies rather than reaching for globals.
- **Tests:** behavioral and deterministic; **test-first** (failing test → minimal code → refactor while green); race / shuffle / parallel green; cover failure paths; fixtures over embedded config; regression-test every fix.
- **AI / model features:** treat model output and retrieved context as untrusted; enforce structured outputs; least-privilege tool calls with a human gate on destructive actions; version prompts / models and gate changes on evals.
- **Supply chain:** pin CI actions by SHA; scan dependencies (vulnerabilities + licenses); sign release artifacts; attach SBOM and provenance.
- **Keep code current:** use current idioms and standards, not old habits — verify the dependency is maintained, the stdlib doesn't already cover it, and no open CVE applies.
- **Consistency and idioms:** project-wide consistency and idiomatic Rust outrank symbol parity. Match shared behavior and wire contracts, not another language's internal shapes.

Use the matching [skill](skills/README.md). Review affected behavior and its dependencies; delegate only when the user requests it. Scope `make`/`cargo` validation to changed crates while implementing.

## Build, Test, and Lint

Requires:
Rust 1.97+ (declared by workspace `rust-version`; development toolchain pinned via `rust-toolchain.toml`).

```bash
make check              # Full validation: fmt-check + lint + build + test
make build              # Build workspace (C=<crate> for specific crate)
make test               # Run tests (C=<crate>, T=<pattern>)
make test-coverage      # LCOV coverage report
make lint               # Clippy with -D warnings
make fmt                # Format with rustfmt
make fmt-check          # Check formatting without modifying
make doc                # Build docs with -D warnings
make deny               # cargo-deny (licenses, advisories, sources)
```

## Crate Structure

Cargo workspaces are split by role:

- `core/rskit-<name>/` — foundation crates and the `rskit` facade
- `contrib/<domain>/<name>/` —
  adapter crates grouped by domain (`storage`, `cache`, `messaging`, `inference`, `llm`, `media`, `vectorstore`)
- `examples/<name>/` — demos and sample applications

Core crates cover the shared foundations and cross-cutting modules (for example `errors`, `config`, `logging`, `bootstrap`, `provider`, `pipeline`, `resilience`, `worker`, `server`, `validation`, `http`, `di`, `auth`, `observability`, `authz`, `discovery`, `security`, `process`, `media`, `cli`, and `dataset`). Adapter crates live under `contrib/` by domain, such as `contrib/storage/s3`, `contrib/messaging/kafka`, or `contrib/media/ffmpeg`.

The facade crate (`rskit`) re-exports core crates and exposes adapter integrations via feature flags.

When adding a new foundation crate: create it under `core/rskit-<name>/`, add it to `core/Cargo.toml`, inherit workspace package metadata, add `#![warn(missing_docs)]`, and wire it into the facade as appropriate. When adding an adapter crate, place it under `contrib/<domain>/<name>/` and make sure it is covered by the matching `contrib/Cargo.toml` workspace member pattern.

## Code Style

- `cargo fmt` (`rustfmt.toml`: edition 2024, max_width 100) + `cargo clippy` (`clippy.toml`: msrv 1.97)
- `lib.rs`/`mod.rs` contain declarations and re-exports only; `make structure` enforces this through `scripts/sg-rules/declare-only-aggregator.yml`. Use concern-named modules. Review mixed-concern files around 300–400 non-test code lines, or modules with roughly 10+ files spanning 2–3 separable groups. These are advisory signals, not size limits; split only where ownership becomes clearer.
- `#![warn(missing_docs)]` on all crates
- `#[must_use]` on all `with_*` builder methods
- `#[non_exhaustive]` on public enums that may grow
- `parking_lot::Mutex` instead of `std::sync::Mutex`
- No `unsafe` without `// SAFETY:` comment
- No `unwrap()` / `expect()` in library code (tests OK)
- `AppResult<T>` alias for error handling throughout
- Use builders or parameter structs for cohesive request/options/state inputs, especially same-typed positional arguments. Group by meaning, not line length; retain distinct arguments when grouping would obscure the API.
- Conventional Commits: `feat`, `fix`, `docs`, `refactor`, `test`, `chore`

## Documentation

- Write concise, task-first explanations. Preserve contracts, safety, useful examples, and release history; remove repetition and implementation narration. Skills link canonical procedures.
- Write Markdown paragraphs as natural, continuous source lines. Do not hard-wrap prose to a column limit or insert source newlines for visual presentation; Markdown renderers handle viewport-aware wrapping. Keep intentional structure such as paragraph breaks, headings, lists, blockquotes, tables, mermaid diagrams, and fenced or indented code blocks.
- Apply the same rule to prose in `//!`/`///` rustdoc and `//` comments: do not introduce arbitrary column-based breaks. Preserve rustdoc formatting conventions for code examples, directives, lists, and tables. The `rustfmt` `max_width` limit is for code, not prose.
- Comments and rustdoc describe the code as it is now — not history, plans, or the process that produced it.

## Key Patterns

- **Typestate lifecycle**: `App<S, C>` ensures compile-time lifecycle ordering.
- **Error handling**: `AppError` with `ErrorCode` enum, RFC 9457 problem details, and lightweight HTTP status metadata. gRPC status mapping belongs in `rskit-grpc`, not `rskit-errors`.
- **Component lifecycle**: `Component` trait with `start/stop/health`, Registry ordering.
- **Provider**: `RequestResponse`, `Stream`, `Sink`, `Duplex` traits with a tower bridge.
- **Pipeline**: `futures::Stream` extension operators (map, filter, fan_out, window, batch, parallel).
- **Testing**: time-dependent tests use `tokio::time::pause()`/`advance()`, never `std::thread::sleep`. Env-var tests hold `parking_lot::Mutex<()>` guard.
