# rskit

Rust infrastructure toolkit providing foundational crates for service development.
Mirrors gokit (Go) and pykit (Python) in module structure and naming.

## Engineering principles

Shared engineering baseline — apply to all work here:

- **Phases:** discover → decide (Redesign / Align / Enhance / Drop / Leave) → implement completely → validate.
  Prefer root-cause redesign over symptom patches; no compatibility shims in pre-stable code.
- **Layering & reuse:** explicit, acyclic dependency direction — lower layers never import higher.
  Reuse or enhance the canonical owner before writing new code;
  never duplicate shared concerns (errors, config, logging, auth, retries, observability, HTTP, registries).
  Consult [`docs/CONCERN-OWNERS.md`](../docs/CONCERN-OWNERS.md) for the canonical owner of each shared concern (formats → `rskit-codec`, helpers → `rskit-util`, paths → `rskit-fs`, …) before writing new code.
- **APIs:** typed and minimal;
  no broad `Any` / `interface{}` / unchecked `unknown` in public surfaces;
  actionable typed errors that preserve cause.
- **Errors & resilience:** no panics / unwrap or swallowed errors on runtime paths;
  no success-shaped fallbacks; timeout every remote call;
  bounded jittered retries for idempotent ops only; circuit-break and degrade gracefully.
- **Concurrency:** every task has ownership, cancellation, timeout, and shutdown;
  bound queues / buffers / concurrency with documented backpressure; drain on shutdown.
- **Security & privacy:** validate at every trust boundary; least-privilege and secure-by-default;
  parameterized queries and argv-only subprocess; tokens in headers, not query strings;
  current crypto only; minimize, redact, and retention-bound sensitive data.
- **Composition:** explicit injected registries and config-driven selection;
  no import-time side effects, no mutable global registries;
  inject logger / tracer / policies rather than reaching for globals.
- **Tests:** behavioral and deterministic; race / shuffle / parallel green; cover failure paths;
  fixtures over embedded config; regression-test every fix.
- **AI / model features:** treat model output and retrieved context as untrusted;
  enforce structured outputs; least-privilege tool calls with a human gate on destructive actions;
  version prompts / models and gate changes on evals.
- **Supply chain:** pin CI actions by SHA; scan dependencies (vulnerabilities + licenses);
  sign release artifacts; attach SBOM and provenance.
- **Keep code current:** use current idioms and standards, not old habits —
  verify the dependency is maintained, the stdlib doesn't already cover it, and no open CVE applies.

Standing,
re-runnable development skills encoding this baseline live in [`.github/skills/`](skills/README.md)
— the `review` skill runs the review passes in a fresh, clean-context agent after every change set
and before releases; `create-branch`, `create-plan`, `apply-plan`, `apply-step`, `commit`,
`create-pr`, `fix-reviews`, `validate`, `new-crate`, `new-backend`, `release`,
and `sibling-parity` cover the rest of the workflow. Validation is driven through `make`/`cargo`,
scoped to the changed crate(s).

## Build, Test, and Lint

Requires:
Rust 1.91+ (declared by workspace `rust-version`; development toolchain pinned via `rust-toolchain.toml`).

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

Core crates cover the shared foundations
and cross-cutting modules (for example `errors`, `config`, `logging`, `bootstrap`, `provider`, `pipeline`, `resilience`, `worker`, `server`, `validation`, `http`, `di`, `auth`, `observability`, `authz`, `discovery`, `security`, `process`, `media`, `cli`, and `dataset`).
Adapter crates live under `contrib/` by domain, such as `contrib/storage/s3`,
`contrib/messaging/kafka`, or `contrib/media/ffmpeg`.

The facade crate (`rskit`) re-exports core crates
and exposes adapter integrations via feature flags.

When adding a new foundation crate: create it under `core/rskit-<name>/`,
add it to `core/Cargo.toml`, inherit workspace package metadata, add `#![warn(missing_docs)]`,
and wire it into the facade as appropriate. When adding an adapter crate,
place it under `contrib/<domain>/<name>/`
and make sure it is covered by the matching `contrib/Cargo.toml` workspace member pattern.

## Code Style

- `cargo fmt` (`rustfmt.toml`: edition 2024, max_width 100) + `cargo clippy` (`clippy.toml`: msrv 1.91)
- `lib.rs`/`mod.rs` are **declare-only** (submodule declarations + re-exports; no logic or private items)
  — split crate logic into concern-named modules.
  Enforced by the `ast-grep` rule `scripts/sg-rules/declare-only-aggregator.yml` (`make structure`).
- `#![warn(missing_docs)]` on all crates
- `#[must_use]` on all `with_*` builder methods
- `#[non_exhaustive]` on public enums that may grow
- `parking_lot::Mutex` instead of `std::sync::Mutex`
- No `unsafe` without `// SAFETY:` comment
- No `unwrap()` / `expect()` in library code (tests OK)
- `AppResult<T>` alias for error handling throughout
- Treat a long positional argument list as a design signal:
  when several arguments form a cohesive group (a request context, options, run state, …),
  prefer a builder or parameter struct (`#[derive(Default)]`) for call-site clarity
  and non-breaking extension. This is guidance for better structure, not a hard limit —
  never force artificial grouping or bundle genuinely distinct arguments when doing
  so would hurt readability or maintainability.
- Conventional Commits: `feat`, `fix`, `docs`, `refactor`, `test`, `chore`

## Documentation

- Prose uses **semantic line breaks at a 100-column soft ceiling**.
  Break only at meaningful boundaries: at **sentence boundaries first**,
  then at **clause boundaries** (after a comma/semicolon/colon, around an em or en dash, or before a coordinating conjunction) when a single sentence still exceeds 100 columns.
  **Never break inside a clause** to hit a column —
  a clause with no legal break point may exceed 100 rather than break mid-clause.
  The 100 columns is a *soft ceiling*, not a hard wrap.
  This applies identically to **Markdown prose, `//!`/`///` rustdoc, and `//` comments**;
  fenced/indented code blocks, tables, mermaid diagrams, lists, blockquotes, YAML frontmatter,
  decorative dividers, and in-comment code examples are preserved verbatim.
  The `rustfmt` `max_width` limit is for *code*, not prose.
- Comments and `///` docs describe the code as it is now — not history, plans,
  or the process that produced it.

## Key Patterns

- **Typestate lifecycle**: `App<S, C>` ensures compile-time lifecycle ordering.
- **Error handling**: `AppError` with `ErrorCode` enum, RFC 9457 problem details,
  and lightweight HTTP status metadata. gRPC status mapping belongs in `rskit-grpc`,
  not `rskit-errors`.
- **Component lifecycle**: `Component` trait with `start/stop/health`, Registry ordering.
- **Provider**: `RequestResponse`, `Stream`, `Sink`, `Duplex` traits with a tower bridge.
- **Pipeline**:
  `futures::Stream` extension operators (map, filter, fan_out, window, batch, parallel).
- **Testing**: time-dependent tests use `tokio::time::pause()`/`advance()`,
  never `std::thread::sleep`. Env-var tests hold `parking_lot::Mutex<()>` guard.
