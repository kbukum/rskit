# rskit

Rust infrastructure toolkit providing foundational crates for service development. Mirrors gokit (Go) and pykit (Python) in module structure and naming.

## Build, Test, and Lint

Requires: Rust 1.85+ (enforced via `rust-toolchain.toml`).

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

Cargo workspace with 31 crates in `crates/`, organized by phase:

| Phase | Crates |
|-------|--------|
| Core | rskit (facade), errors, config, logging, bootstrap, provider, pipeline, resilience, worker, server |
| Foundation | validation, http, di, auth |
| Adapters | database, cache, messaging |
| Platform | observability, authz, discovery |
| Specialist | testutil, sse, dag, llm |
| Media & File | file, media, media-ffmpeg, media-image |
| CLI & Data | cli, dataset, bench |

The facade crate (`rskit`) re-exports all sub-crates via feature flags.

When adding a new crate: create under `crates/rskit-<name>/`, add to workspace members, inherit workspace package metadata, add `#![warn(missing_docs)]`, wire into facade.

## Code Style

- `cargo fmt` (`rustfmt.toml`: edition 2024, max_width 100) + `cargo clippy` (`clippy.toml`: msrv 1.85)
- `#![warn(missing_docs)]` on all crates
- `#[must_use]` on all `with_*` builder methods
- `#[non_exhaustive]` on public enums that may grow
- `parking_lot::Mutex` instead of `std::sync::Mutex`
- No `unsafe` without `// SAFETY:` comment
- No `unwrap()` / `expect()` in library code (tests OK)
- `AppResult<T>` alias for error handling throughout
- Conventional Commits: `feat`, `fix`, `docs`, `refactor`, `test`, `chore`

## Key Patterns

- **Typestate lifecycle**: `App<S, C>` ensures compile-time lifecycle ordering.
- **Error handling**: `AppError` with `ErrorCode` enum, HTTP + gRPC status mapping.
- **Component lifecycle**: `Component` trait with `start/stop/health`, Registry ordering.
- **Provider**: `RequestResponse`, `Stream`, `Sink`, `Duplex` traits with tower middleware.
- **Pipeline**: `futures::Stream` extension operators (map, filter, fan_out, window, batch, parallel).
- **Testing**: time-dependent tests use `tokio::time::pause()`/`advance()`, never `std::thread::sleep`. Env-var tests hold `parking_lot::Mutex<()>` guard.
