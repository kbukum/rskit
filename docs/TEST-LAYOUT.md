# Test layout

The single source of truth for **where tests live** in rskit and **how they are organized**. It describes rskit as it already is, so the convention can be applied consistently as crates are hardened. This is the Rust-idiomatic sibling of gokit's `docs/TEST-LAYOUT.md` and pykit's equivalent: the same three tiers, expressed the Rust way.

The reuse rule for shared test tooling — fakes, harnesses, golden helpers, assertions belong in `core/rskit-testutil`, never hand-rolled in a `#[cfg(test)]` block — is governed together with the production concern owners in [`CONCERN-OWNERS.md`](CONCERN-OWNERS.md). This document covers test placement only.

## The three tiers

rskit tests fall into three tiers by scope and visibility. There is **no mandate of one test file per source file** and no coverage-padding split — a tier is chosen by what the test proves, not to hit a number.

### Tier 1 — unit tests (co-located, white-box)

In-crate tests that exercise a single concern, including private behavior. They live in an inline `#[cfg(test)] mod tests { … }` **at the bottom of the concern file** they cover (`foo.rs`).

- When the inline module grows large, extract it to a sibling child module — `#[cfg(test)] mod tests;` declared in `foo.rs`, body in `foo/tests.rs` (or `foo_tests.rs` beside it) — de-indented one level. The aggregator that declares it stays declare-only.
- Deterministic: pause/advance time (`tokio::time::pause()`/`advance()`), never `std::thread::sleep`; use `TestWorkspace`/tempdirs (no real network or filesystem); hold the env-var `parking_lot::Mutex` guard for env-dependent tests. Green under the workspace's race/shuffle/parallel settings.

### Tier 2 — module tests (sibling `tests.rs`)

Tests that pin an aggregate module's behavior across several of its concern files live in a sibling `tests.rs` beside the module's `mod.rs`, declared `#[cfg(test)] mod tests;`. This keeps a declare-only `mod.rs` (no inline test module inside the aggregator) while grouping the module's cross-file tests in one place.

- Same determinism rules as tier 1.
- Use tier 2 when a test spans multiple sibling concern files of the same module; use tier 1 when it targets a single concern file.

### Tier 3 — integration tests (crate `tests/`, black-box)

Tests that drive a crate **through its public API only** live in the crate's top-level `tests/` directory (each file is its own crate, black-box by construction). This is where a `contrib/` adapter exercises the backend-agnostic `core` crate's contract, and where cross-crate wiring is proven.

- A test that needs a live service (broker, container, network, real SDK credentials) is marked `#[ignore]` (contrib integration tests) or guarded so the default `cargo nextest run` stays hermetic — never failing a developer who lacks the backend.
- Backend-agnostic **core** crates are exercised from their `contrib/` adapter's `tests/`; coverage attributes back via the workspace coverage tooling. A core crate reading low in isolation is often covered from its adapter — **re-measure before assuming a gap** (`make coverage`, read `target/coverage/summary.json`).

## Where shared test tooling lives

Fakes, harnesses, golden-file helpers, workspace/tempdir setup, and assertions are a **shipped product** (`core/rskit-testutil`: `FakeComponent`, `MockProvider`, `Golden`, `TestWorkspace`, `assert_ok`/`assert_err_code`, `TestEvent`, `TestAppConfig`, `CurrentDirGuard`, …), not throwaway scaffolding.

- Never hand-roll a one-off fake in a `#[cfg(test)]` block when a shared helper exists or the fake should live in `rskit-testutil`. When a test needs a new fake/harness, **add or extend it in `rskit-testutil` (or the domain's testutil module) and reuse it** — added once, reused everywhere (see the reuse dimension in `tmp/tdd-hardening/README.md`).
- `rskit-testutil` keeps a declare-only `lib.rs`, `#![warn(missing_docs)]`, and accurate rustdoc for every promoted helper.

## Non-goals

- No one-test-module-per-source-file mandate; no coverage-padding test files.
- No forced black-box split — choose the tier by what the test proves.
- Precise terminology: **unit** (tier 1) / **module** (tier 2) / **integration** (tier 3). Use these names in reviews and skills.

## Enforcement

Test placement is a **review pass**, not a hard gate — see `.github/skills/review/references/00-structure-placement.md` (Test placement). The `declare-only-aggregator` ast-grep rule (`make structure`) already guards aggregators against inline test modules, so tier-2 tests must be a sibling `tests.rs`. Nothing here blocks CI beyond that existing structure guard; the review pass and reviewer judgment carry it.
