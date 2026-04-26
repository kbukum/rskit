# Contributing to rskit

Thank you for your interest in contributing! This document explains how to get
started, what we expect from contributors, and how the review process works.

---

## Table of Contents

- [Code of Conduct](#code-of-conduct)
- [Getting Started](#getting-started)
- [Development Setup](#development-setup)
- [Making Changes](#making-changes)
- [Testing](#testing)
- [Commit Style](#commit-style)
- [Pull Request Process](#pull-request-process)
- [Adding a New Crate](#adding-a-new-crate)
- [Crate Conventions](#crate-conventions)

---

## Code of Conduct

Be respectful, constructive, and patient. We follow the
[Contributor Covenant v2.1](https://www.contributor-covenant.org/version/2/1/code_of_conduct/).

---

## Getting Started

1. [Fork](https://github.com/kbukum/rskit/fork) the repository.
2. Clone your fork:

   ```sh
   git clone https://github.com/<your-username>/rskit.git
   cd rskit
   ```

3. Set the upstream remote:

   ```sh
   git remote add upstream https://github.com/kbukum/rskit.git
   ```

---

## Development Setup

**Minimum Rust version:** 1.85 (enforced by `rust-toolchain.toml`).

```sh
# Install the pinned toolchain + components
rustup show

# Build everything
cargo build --workspace

# Run all tests
cargo test --workspace

# Lint (must be clean)
cargo clippy --workspace -- -D warnings

# Format check
cargo fmt --check
```

Optional but recommended:

```sh
# Supply-chain audit
cargo install cargo-deny
cargo deny check

# Documentation
cargo doc --workspace --no-deps --open
```

---

## Making Changes

1. Create a feature branch from `main`:

   ```sh
   git checkout -b feat/my-feature
   ```

2. Make the smallest change that achieves the goal. Avoid unrelated clean-up in
   the same PR — file a separate issue/PR for it.

3. Keep public APIs additive and backward-compatible unless the change is
   intentionally breaking (discuss first).

4. Update `CHANGELOG.md` under `## [Unreleased]` with a brief description of
   what you added, changed, or fixed.

---

## Testing

- Every public function and trait impl should have at least one test.
- Time-dependent tests **must** use `tokio::time::pause()` / `tokio::time::advance()` —
  never `std::thread::sleep`.
- Env-var tests must hold a `static parking_lot::Mutex<()>` guard to prevent
  cross-test pollution (see `rskit-config/src/loader.rs` for the pattern).
- Tests that require a live service (e.g., gRPC integration tests) go in
  `crates/<crate>/tests/` and are gated with `#[ignore]` plus a doc comment
  explaining what service is needed.

Run the full suite before submitting:

```sh
cargo test --workspace
```

---

## Commit Style

We follow [Conventional Commits](https://www.conventionalcommits.org/en/v1.0.0/):

```
<type>(<scope>): <short summary>

[optional body]

[optional footer(s)]
```

Types: `feat`, `fix`, `docs`, `style`, `refactor`, `test`, `chore`, `ci`.

Scope is the crate short name, e.g. `errors`, `resilience`, `worker`.

Examples:

```
feat(resilience): add sliding-window rate limiter variant
fix(pipeline): prevent debounce from emitting stale item on cancel
docs(worker): document EventKind variants
chore(ci): pin cargo-deny to 0.16
```

---

## Pull Request Process

1. Push your branch and open a PR against `main`.
2. Fill in the PR template completely.
3. Ensure CI passes (clippy `-D warnings`, `cargo test`, doc build).
4. Request a review from a maintainer.
5. Address review comments in follow-up commits (do not force-push after review
   has started unless asked).
6. A maintainer will squash-merge once approved.

---

## Adding a New Crate

1. Create the crate under `crates/rskit-<name>/` with `cargo new --lib`.
2. Add it to `[workspace.members]` in the root `Cargo.toml`.
3. Inherit workspace metadata (`version.workspace = true`, etc.).
4. Add `#![warn(missing_docs)]` to `src/lib.rs`.
5. Wire it into the `rskit` facade crate.
6. Add an entry to the crate map in `README.md`.
7. Open a tracking issue describing the API surface before implementing, so the
   design can be discussed early.

---

## Crate Conventions

| Convention | Requirement |
|---|---|
| `#![warn(missing_docs)]` | All crates |
| `#[must_use]` on all `with_*` builder methods | All crates |
| `#[non_exhaustive]` on public enums | All public enums that may grow |
| `parking_lot::Mutex` instead of `std::sync::Mutex` | All crates (non-poisoning, consistent) |
| No `unsafe` without a `// SAFETY:` comment | All crates |
| No `unwrap()` / `expect()` in library code | All crates (tests are fine) |
| `tokio::time::pause()` for time-based tests | `rskit-pipeline`, `rskit-resilience` |
| `#[allow(async_fn_in_trait)]` for public traits with default impls | As needed |

---

## Related Documents

- [CODE_OF_CONDUCT.md](CODE_OF_CONDUCT.md) — Contributor Covenant v2.1
- [SECURITY.md](SECURITY.md) — vulnerability disclosure & supply-chain
- [GOVERNANCE.md](GOVERNANCE.md) — roles, decision making, sibling-parity contract
- [MAINTAINERS.md](MAINTAINERS.md) — current maintainers & areas
- [CHANGELOG.md](CHANGELOG.md) — release history
- [docs/RELEASING.md](docs/RELEASING.md) — release process
- [docs/VERSIONING.md](docs/VERSIONING.md) — versioning rules
- [docs/policy/SEMVER.md](docs/policy/SEMVER.md) · [docs/policy/DEPRECATION.md](docs/policy/DEPRECATION.md)
- [docs/adr/](docs/adr/) — Architecture Decision Records

### Sibling-parity reminder

Public abstractions (`AppError`, `Component`, `Provider`, `Pipeline`, lifecycle
hooks) are mirrored across [gokit](https://github.com/kbukum/gokit),
[rskit](https://github.com/kbukum/rskit), and
[pykit](https://github.com/kbukum/pykit). When you change one of these
surfaces here, please open tracking issues in the sibling repos so the change
can be evaluated for parity.
