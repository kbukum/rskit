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

Cargo workspace split by role:

- `core/rskit-<name>/` — foundation crates and the `rskit` facade
- `contrib/<domain>/<name>/` — adapter crates grouped by domain (`storage`, `cache`, `messaging`, `inference`, `llm`, `media`, `vectorstore`)
- `examples/<name>/` — demos and sample applications

Core crates cover the shared foundations and cross-cutting modules (for example `errors`, `config`, `logging`, `bootstrap`, `provider`, `pipeline`, `resilience`, `worker`, `server`, `validation`, `http`, `di`, `auth`, `observability`, `authz`, `discovery`, `security`, `process`, `media`, `cli`, and `dataset`). Adapter crates live under `contrib/` by domain, such as `contrib/storage/s3`, `contrib/messaging/kafka`, or `contrib/media/ffmpeg`.

The facade crate (`rskit`) re-exports core crates and exposes adapter integrations via feature flags.

When adding a new foundation crate: create it under `core/rskit-<name>/`, add it to workspace members, inherit workspace package metadata, add `#![warn(missing_docs)]`, and wire it into the facade as appropriate. When adding an adapter crate, place it under `contrib/<domain>/<name>/`.

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
- **Provider**: `RequestResponse`, `Stream`, `Sink`, `Duplex` traits with a tower bridge.
- **Pipeline**: `futures::Stream` extension operators (map, filter, fan_out, window, batch, parallel).
- **Testing**: time-dependent tests use `tokio::time::pause()`/`advance()`, never `std::thread::sleep`. Env-var tests hold `parking_lot::Mutex<()>` guard.
