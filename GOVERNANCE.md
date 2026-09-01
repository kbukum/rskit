# Governance

This document describes how decisions are made in the rskit project.

## Project Status

rskit is **pre-stable** (`v0.x`).
Backward compatibility is **not** guaranteed between `v0.x` releases;
breaking changes are acceptable when they yield a cleaner long-term design.
See [CHANGELOG.md](CHANGELOG.md) for the full breaking-change history.

## Sibling-Parity Contract

rskit is part of a sibling pair that intentionally mirrors module structure, naming, and patterns:

- [`kbukum/gokit`](https://github.com/kbukum/gokit) — Go
- [`kbukum/rskit`](https://github.com/kbukum/rskit) — Rust (this repo)

When a public abstraction (`AppError`, `Component`, `Provider`, `Pipeline`, lifecycle hooks, error codes, configuration semantics) is changed in one sibling, the same change should be evaluated for the other. Drift is treated as a finding and tracked in cross-sibling issues.

## Roles

### Contributors

Anyone who opens an issue or pull request is a contributor.
Contributors are expected to follow the [Code of Conduct](CODE_OF_CONDUCT.md)
and the [Contribution Guide](CONTRIBUTING.md).

### Reviewers

Reviewers are contributors who have shown sustained engagement
and are empowered to approve pull requests in specific areas of the code.
Reviewer assignments are recorded in [.github/CODEOWNERS](.github/CODEOWNERS).

### Maintainers

Maintainers have merge rights and are responsible for the long-term direction of the project.
The current list is in [MAINTAINERS.md](MAINTAINERS.md).

## Decision Making

For routine changes (bug fixes, small features), a single maintainer approval is sufficient. For changes that affect multiple crates or change a public API, the contributor is encouraged to open a discussion or RFC issue first.

For significant architectural changes (e.g. introducing a new sub-crate, removing a public crate, changing the layering rules, changing the release process), at least two maintainers must approve. If maintainers disagree, the proposal is deferred until consensus is reached or a clear path forward is documented in an [ADR](docs/adr/).

## Release Process

Releases are cut by maintainers following [docs/RELEASING.md](docs/RELEASING.md).
Each release MUST be accompanied by a CHANGELOG entry that follows the [Keep a Changelog](https://keepachangelog.com/en/1.1.0/) format. Maintainers keep a single `[Unreleased]` heading and rotate it on release per [docs/RELEASING.md](docs/RELEASING.md).

## Security Issues

Security issues follow the dedicated process in [SECURITY.md](SECURITY.md)
and are not handled via the normal issue tracker.

## Amendments

This document may be amended via pull request.
Amendments require approval from a majority of current maintainers.
