# Rust Review — Plan, Clarify, Apply

An alternative orchestrator to [`review-changes.md`](./review-changes.md) / [`review-project.md`](./review-project.md): instead of sequencing the 00–07 lenses, it fans the review out into **parallel subagent passes by Rust concern**, then plans and applies fixes. Use it when you want one driver to take a change from review through to merged fixes.

Run each pass as a **separate subagent with clean context**. The orchestrator (this file) sequences them and collects findings. Do not concatenate passes into one prompt.

Mode is either **changes** (a diff: branch, commit range, `HEAD~1`) or **project** (whole tree, no diff). State the mode up front.

> The focused 00–07 files hold the canonical, rskit-specific checks (placement, canonical-owner reuse, security/privacy, supply chain, comments). This file is the *driver*; when a pass below needs the full rskit rule for a lens, defer to the matching focused file rather than duplicating it.

---

## Phase 1 — Scope

1. `git status`, `git diff --stat`, `git diff` (changes mode) or `ls core contrib examples` + dependency map (project mode). Preserve uncommitted changes; integrate on top, never discard.
2. List the surface to review: changed crates (changes mode) or chosen crates/workspace (project mode). Note cross-cutting touches: a core crate's public surface fans out to the `rskit` facade, the other core crates, every `contrib/` adapter, and downstream consumers (pykit/gokit parity, Toven). Also flag workspace-`Cargo.toml` edits, shared error types, public re-exports, `[lints]`.
3. Determine which passes apply via the triggers below. Skip non-applicable passes explicitly in the final report.

The reviewer judges code as written, against the rules below and the baseline in [`.github/copilot-instructions.md`](../../../copilot-instructions.md). PR descriptions, commit messages, or plan/ADR docs are scope hints only — never justifications.

## Phase 2 — Passes

Run **A first** (cheap, gates the rest). Then **B–F in parallel** where independent. Then **G last** (cross-references everything).

Each subagent receives: its scope, the pass spec below, and nothing else. Each returns findings in the shared format. Scope `cargo`/`make` to the touched crate(s) with `C=<crate>` or `-p <crate>`; the unscoped workspace gates are slow across 50+ crates and belong to sign-off/CI.

### Pass A — Mechanical (always runs)

Tool output only, no judgment. Use rskit's real gates:

```bash
make fmt-check                                   # cargo fmt --check, whole tree (fast)
make lint C=<crate>                              # clippy -D warnings, scoped (or W=<workspace>)
make deny                                        # cargo-deny + L7-edges + workspace-dep-sync + topology + public-api
make check-public-api                            # only if a public surface changed
make doc C=<crate>                               # rustdoc -D warnings, if public docs in scope
```

Report pass/fail per command with the first failure block verbatim.

### Pass B — Correctness

**Scope:** all in-scope `.rs` files.

Check: ownership and lifetimes; partial moves; `unwrap`/`expect` or swallowed errors (`let _ = …` without comment) on fallible runtime paths (tests excepted); no success-shaped fallback that masks failure; error context preserved through `?`/`From`/`map_err` as rskit `AppError`/`AppResult` with `ErrorCode` and cause intact; panics only on documented invariants; every `unsafe` block carries a substantive `// SAFETY:` comment (Edition 2024 `#[unsafe(no_mangle)]` wrapping where required); resource cleanup on every return path including errors; `Drop` impls don't panic. *(Canonical owner: pass [`01`](./01-canonical-reuse.md).)*

Skip if: scope is docs-only or config-only.

### Pass C — Concurrency

**Scope:** files importing `tokio`, `std::thread`, `std::sync`, `futures`, `rayon`, `parking_lot`, or containing `async fn`/`.await`.

Check: every spawned task has clear ownership, cancellation, timeout, and shutdown; no `MutexGuard`/`RefCell` borrow held across `.await`; no `block_on` under tokio; CPU/blocking work uses `spawn_blocking` so draining continues; structured concurrency via `JoinSet`/`rskit-worker` over loose `spawn`; channels/queues/buffers are **bounded with documented backpressure** and components **drain on shutdown** (an unbounded channel or a task with no cancellation path is a **blocker**); `parking_lot::Mutex`, never `std::sync::Mutex`; `Send`/`Sync` not added unsoundly. Time-dependent paths are testable via `tokio::time::pause()`/`advance()`, not wall-clock sleeps.

Skip if: no async/threading surface in scope.

### Pass D — Composition and lifecycle

**Scope:** registries, `Component`/`Registry` impls, `App<S, C>` typestate wiring, provider/adapter construction, anything wiring dependencies together.

Check: registries and policies are **explicitly injected**, selection is config-driven; **no import-time side effects, no mutable global registry**, no reaching for a global logger/tracer — inject them (a `lazy_static!`/`static mut`/`once_cell::sync::Lazy` registry or init-on-import is a **blocker**); `Component` lifecycle (`start`/`stop`/`health`) is honored with Registry ordering and drain-on-stop; typestate lifecycle ordering (`App<S, C>`) is not bypassed; the `rskit` facade only re-exports — behavior added directly to the facade is misplaced; adapters are exposed behind a feature flag, not unconditionally. *(Placement: pass [`00`](./00-structure-placement.md); composition principle: pass [`02`](./02-principles.md).)*

Skip if: no composition/lifecycle/registry surface in scope.

### Pass E — Security, config, and boundaries

**Scope:** external-facing surfaces (HTTP, process, storage/database/cache adapters, auth, crypto), config loaders, env-var handling, path handling, and docs describing config or env.

Check: untrusted input is validated at every trust boundary before flowing into a query, path, command, or deserialization (an unvalidated path is a **blocker**); parameterized queries only — never string-built SQL; argv-only subprocess execution via `rskit-process`, no shell interpolation of untrusted input; tokens/credentials in headers not query strings, never logged, redacted in errors; current crypto only (no MD5/SHA-1-for-security/ECB/static IV/hard-coded key) routed through `rskit-encryption`/`rskit-security`; unbounded reads of untrusted input get explicit size limits; config keys round-trip (dotted ↔ TOML table) with precedence (CLI > env > file > default) tested; path-shaped values use `PathBuf`/`Path::join` (never hardcoded separators) and `tempfile` over `/tmp/...`; platform-specific behavior uses explicit `#[cfg(...)]` with both branches exercised. *(Full rule: pass [`03`](./03-security-privacy.md).)*

Skip if: no security-sensitive, config, env, path, or cross-platform code in scope.

### Pass F — API surface and dependencies

**Scope:** `lib.rs`, `mod.rs`, `Cargo.toml`, anything changing `pub` items.

Check: new `pub` items intentional (prefer `pub(crate)`; `make check-public-api` backs this); no broad `Any`/`Box<dyn Any>`/stringly-typed escape hatch on a public surface; `&str` over `String`, `&[T]` over `Vec<T>` in parameters where ownership isn't needed; `#[non_exhaustive]` on public enums/structs that may grow; `#[must_use]` on `with_*` builders and result-like types; new deps justified (maintained, no open CVE, not duplicating a core crate or std — currency, pass [`01`](./01-canonical-reuse.md)), `default-features = false` where applicable, shared versions consistent across workspaces (`make check-workspace-deps-sync`); `Cargo.lock` committed and consistent; `rust-version`/MSRV 1.91 declared; `edition = "2024"`; lints live in the `[lints]` table; a new crate is wired into `core/Cargo.toml` (or the `contrib/Cargo.toml` member pattern), inherits workspace metadata, and carries `#![warn(missing_docs)]`.

Skip if: no public items, deps, or `Cargo.toml` in scope.

### Pass G — Tests, docs, semantics (runs last)

**Scope:** the in-scope code plus findings from A–F.

Check: behavioral code in scope has tests covering it (changes mode: in the same diff; project mode: anywhere in the tree); bug fixes have a regression test that fails without the fix; failure paths asserted, not just happy paths; tests are **green under race / shuffle / parallel** and depend on no wall clock, network, or working directory unless intentional (time uses `tokio::time::pause()`/`advance()`; env-var tests hold the `parking_lot::Mutex<()>` guard; cwd tests use `rskit-testutil`'s `CurrentDirGuard`); fixtures over large inline config; an operation does what its name implies; `#![warn(missing_docs)]` satisfied with `//!` crate docs and `///` item docs (incl. `# Errors`/`# Panics`) that match implemented behavior; doc examples compile; comments describe the code as it is, not plans/history. *(Full rules: passes [`05`](./05-tests-tdd.md) and [`06`](./06-docs-supply-chain.md); comment hygiene: pass [`07`](./07-comments-rustdoc.md).)*

Always runs.

## Phase 3 — Consolidate

Orchestrator collects findings into one table:

```
pass | severity (blocker/should-fix/nit) | file:line | finding | suggested fix
```

Severity rule: **blocker** = principle violation, behavior is wrong, or a contract is broken (see [`SKILL.md`](../SKILL.md) for the full definition). Otherwise should-fix or nit.

Group by file in the final report. State explicitly any pass that was **skipped** (with the trigger that failed) and any pass that was **deferred** (with reason).

## Phase 4 — Plan and clarify

Group findings by pass, order by severity. For each group write a one-line fix plan: what changes, where, how it's verified. Flag ambiguities (behavior change vs strict fix, breaking API vs deprecation, doc-only vs behavior-aligning) with a proposed default and the alternative. **Pause for user confirmation before editing.**

## Phase 5 — Apply

After confirmation:

1. Apply fixes in plan order, one pass per commit where reasonable (Conventional Commits: `feat`/`fix`/`docs`/`refactor`/`test`/`chore`).
2. Re-run the matching pass's validation after each fix, scoped to the touched crate(s). Stop and report if anything fails.
3. Final step: re-run Pass A across the in-scope crates.

## Reviewer notes

- Code judges itself. External narrative (PR description, commit message, plan/ADR doc) is scope only, not justification.
- Detection commands (`rg`, `cargo`, `make`) are loaded by the subagent when it searches, not held in the resident prompt.
- If scope is trivial (docs-only, single-line fix), run only A and G; skip the rest with explicit reason.
