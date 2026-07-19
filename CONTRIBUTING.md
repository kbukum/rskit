# Contributing to rskit

Thank you for your interest in contributing! This document explains how to get started,
what we expect from contributors, and how the review process works.

---

## Table of Contents

- [Code of Conduct](#code-of-conduct)
- [Getting Started](#getting-started)
- [Prerequisites](#prerequisites)
- [Development Setup](#development-setup)
- [Making Changes](#making-changes)
- [Testing](#testing)
- [Quick Development Workflow](#quick-development-workflow)
- [Commit Style](#commit-style)
- [Pull Request Process](#pull-request-process)
- [Adding a New Crate](#adding-a-new-crate)
- [Crate Conventions](#crate-conventions)

---

## Code of Conduct

Be respectful, constructive, and patient.
We follow the [Contributor Covenant v2.1](https://www.contributor-covenant.org/version/2/1/code_of_conduct/).

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

## Prerequisites

- Install Rust via [rustup](https://rustup.rs/).
  The repo is pinned to a specific toolchain via `rust-toolchain.toml` —
  rustup will automatically download and use the correct version.
- Install Python 3.11+ for repository automation, then run `make setup` to install
  or verify local Cargo tooling.
- **Linux:** Install `mold` linker for faster builds: `sudo apt install mold`
- **Linux:** `clang` is also required as the linker driver when using the documented `mold` setup.
- **macOS:** No additional linker setup needed (uses platform default)

---

## Development Setup

**Minimum Rust version:** 1.91 (declared by workspace `rust-version`).
The repository pins a newer development toolchain in `rust-toolchain.toml`.

```sh
# Install/verify the pinned toolchain, Python runtime, and local Cargo tools
make setup

# Build all split workspaces
make build

# Run all tests
make test

# Lint (must be clean)
make lint

# Format check
make fmt-check
```

Optional but recommended:

```sh
# Try system/release tool setup as well
scripts/setup.sh --system --release

# Documentation
make doc
```

If you use Cargo directly,
pass the owning manifest because the repository intentionally has no root `Cargo.toml`:

```sh
cargo test --manifest-path core/Cargo.toml -p rskit-errors
cargo test --manifest-path contrib/Cargo.toml -p rskit-storage-s3
cargo test --manifest-path examples/Cargo.toml --workspace
```

---

## Making Changes

1. Create a feature branch from `main`:

   ```sh
   git checkout -b feat/my-feature
   ```

2. Make the smallest change that achieves the goal. Avoid unrelated clean-up in the same PR —
   file a separate issue/PR for it.

3. Keep public APIs additive
   and backward-compatible unless the change is intentionally breaking (discuss first).

4. Update `CHANGELOG.md` under `## [Unreleased]` with a brief description of what you added,
   changed, or fixed.

---

## Testing

### Quick Development Workflow

For rapid iteration:
```bash
make help                     # see available targets
make check-fast               # format + lint + build only (~30s)
make test-nextest             # parallel tests via nextest
make test-affected            # test only crates changed vs main
make test-doc                 # run doctests separately
make check                    # full validation before PR
```

For CI-like local testing:
```bash
PROFILE=ci make test-nextest  # with CI profile (retries, no fail-fast)
```

- Every public function and trait impl should have at least one test.
- Time-dependent tests **must** use `tokio::time::pause()` / `tokio::time::advance()` —
  never `std::thread::sleep`.
- Env-var tests must hold a `static parking_lot::Mutex<()>` guard to prevent cross-test pollution (see `rskit-config/src/loader.rs` for the pattern).
- Tests that require a live service (e.g., gRPC integration tests) go in the crate-local `tests/` directory under `core/rskit-<name>/tests/` for foundation crates
  or `contrib/<domain>/<name>/tests/` for adapters,
  and are gated with `#[ignore]` plus a doc comment explaining what service is needed.

Run the full suite before submitting:

```sh
make check
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
fix(stream): prevent debounce from emitting stale item on cancel
docs(worker): document EventKind variants
chore(ci): pin cargo-deny to 0.16
```

---

## Pull Request Process

1. Push your branch and open a PR against `main`.
2. Fill in the PR template completely.
3. Ensure CI passes (format, clippy `-D warnings`, tests, docs, dependency policy).
4. Request a review from a maintainer.
5. Address review comments in follow-up commits (do not force-push after review has started unless asked).
6. A maintainer will squash-merge once approved.

---

## Adding a New Crate

1. Create foundation crates under `core/rskit-<name>/` with `cargo new --lib`,
   or adapter crates under `contrib/<domain>/<name>/`.
2. Add foundation crates to `core/Cargo.toml`
   or adapter crates to the matching `contrib/<domain>/*` workspace pattern in `contrib/Cargo.toml`.
3. Give the crate its own `version` (seeded from the current alpha, e.g. `version = "0.1.0-alpha.1"`),
   inherit the remaining workspace metadata (`edition.workspace = true`, etc.),
   and add any shared dependencies through the owning workspace manifest.
4. Add `#![warn(missing_docs)]` to `src/lib.rs`.
5. Wire it into the `rskit` facade crate.
6. Add or update package documentation in `docs/PACKAGES.md`
   and facade feature documentation when applicable.
7. Open a tracking issue describing the API surface before implementing,
   so the design can be discussed early.

Version bumps and release preparation are maintainer-only work.
Contributors should add `CHANGELOG.md` entries under `[Unreleased]`;
maintainers follow [`docs/VERSIONING.md`](docs/VERSIONING.md)
and [`docs/RELEASING.md`](docs/RELEASING.md) when cutting a release.

---

## Crate Conventions

| Convention | Requirement |
|---|---|
| `#![warn(missing_docs)]` | All crates |
| `#[must_use]` on all `with_*` builder methods | All crates |
| `#[non_exhaustive]` on public enums | All public enums that may grow |
| `parking_lot::Mutex` instead of `std::sync::Mutex` | All crates (non-poisoning, consistent) |
| Async locks | Only when lock ownership must cross `.await` or coordinate async waiters |
| No `unsafe` without a `// SAFETY:` comment | All crates |
| No `unwrap()` / `expect()` in library code | All crates (tests are fine) |
| `tokio::time::pause()` for time-based tests | `rskit-stream`, `rskit-resilience` |
| `#[allow(async_fn_in_trait)]` for public traits with default impls | As needed |

Prefer synchronous `parking_lot` locks for in-memory state that is accessed
and released within a synchronous critical section. Never hold any lock across unrelated I/O;
document the reason when an async lock is intentionally required.

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

Public abstractions (`AppError`, `Component`, `Provider`, lifecycle hooks) are mirrored across [gokit](https://github.com/kbukum/gokit),
[rskit](https://github.com/kbukum/rskit), and [pykit](https://github.com/kbukum/pykit).
When you change one of these surfaces here, please open tracking issues in the sibling repos
so the change can be evaluated for parity.
