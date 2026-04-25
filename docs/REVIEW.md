# rskit — OSS Engineering Review (Aggregated)

**Date:** 2025-11-28
**Reviewer:** Claude Sonnet 4.6 via GitHub Copilot CLI (acting as principal Rust engineer / OSS maintainer reviewer)
**Scope:** Aggregation of four independent dimension audits of the `rskit` workspace (49 crates) into a single OSS-readiness review.
**Source dimension reports** (read in full, every finding preserved with stable IDs):
- `rskit-dim1-code-arch-concurrency.md` — Code Quality / Architecture / Concurrency (CQ/AR/CC)
- `rskit-dim2-security-errors-obs.md` — Security / Errors / Observability (SC/ER/OB)
- `rskit-dim3-testing-perf-lint.md` — Testing / Performance / Lint (TS/PF/LT)
- `rskit-dim4-ci-toolchain-docs-release-hygiene.md` — CI / Toolchain / Docs / Release / Hygiene (CI/TC/DC/RL/RH)
**Tooling baseline logs:** `tooling-rskit.log{,.build,.clippy,.tests,.deny,.audit,.audit.json}`

**Ground rules**
- Every finding from the four dimension reports is preserved verbatim under a new stable ID (`RS-CR-NN` Critical, `RS-HI-NN` High, `RS-ME-NN` Medium, `RS-LO-NN` Low, `RS-NI-NN` Nit). Original dimension IDs are kept in the **Maps-to** column for traceability.
- All `path:Lstart-Lend` citations are reproduced exactly as captured in the dimension reports against `rskit @ HEAD` at the time of audit.
- "OSS-grade" = small open-source maintainers can adopt this crate as a supply-chain dependency without auditing it themselves. The bar is roughly: tokio / tracing / hyper / axum / tonic / clap / rustls.

---

## §1. Executive Summary (10 blunt bullets)

1. **NOT READY for v1.0 — 11 Criticals + ~50 Highs blocking.** The project is internally usable but cannot be cut as a public OSS release in current shape. There is no release pipeline at all (`RS-CR-09` / RL-01), zero git tags, zero GitHub releases, and `cargo publish` will be **rejected** the first time it is attempted because every `[workspace.dependencies]` sibling entry is `path = `-only with no `version =` (`RS-CR-10` / RL-02).

2. **Baseline tooling is red.** `cargo build` ✓, `cargo test --workspace` ✓ (2 470 passed, 0 failed), but `cargo clippy --all-features -- -D warnings` **FAILS with 4 errors** (3× `io_other_error` in `rs-services/rskit-discovery/src/consul.rs:118,187,217`, 1× `explicit_counter_loop` in `rs-services/rskit-media-ffmpeg/src/probe/detect.rs:170`, plus a `field_reassign_with_default` in `rskit-logging/src/otlp.rs:178`). `cargo fmt --all -- --check` **FAILS with 23 hunks** (mostly `rskit-media-ffmpeg`). `cargo deny check` advisories + licenses **FAIL**. `cargo audit` reports **7 RUSTSEC vulnerabilities + 7 advisory warnings** (RUSTSEC-2023-0071 `rsa` Marvin, RUSTSEC-2026-0098/0099/0104 `rustls-webpki` ×2 each, RUSTSEC-2026-0097 `rand` unsound ×3, RUSTSEC-2026-0002 `lru`, RUSTSEC-2024-0436 `paste`, RUSTSEC-2025-0134 `rustls-pemfile`, RUSTSEC-2025-0119 `number_prefix`).

3. **JWT RSA support is vapourware (`RS-CR-02` / SC-01).** `rs-services/rskit-auth/src/jwt.rs` advertises RS256/384/512 in the public enum but the verification path constructs `DecodingKey::from_secret(secret.as_bytes())` for **every** algorithm. Anyone enabling RS256 for an OSS deployment is silently running HMAC over an attacker-controllable PEM blob. Combined with the missing typestate (anyone can also pass an HS-secret with `Validation::new(Algorithm::RS256)` if they wire it themselves), this is a CVE-class issue we ship at v0.1 if not fixed.

4. **gRPC is plaintext on the wire despite what the type system suggests (`RS-CR-03` / SC-02).** `ServerConfig { tls: Option<TlsConfig> }` is parsed and stored, but `rs-services/rskit-server/src/server.rs:142-178` never threads it into the `tonic::transport::Server::builder().tls_config(..)` call. `cargo +nightly check --features tls` shows `TlsConfig` flagged dead-code. Production users who add `[server.tls]` to their TOML believe they are mTLS-protected; they are not.

5. **HttpServer panics inside detached `tokio::spawn` (`RS-CR-04` / SC-03 / CQ-02).** `rs-services/rskit-http/src/server.rs:88-102` does `tokio::spawn(async move { axum::serve(listener, app).await.unwrap() })` and immediately returns. Bind errors and runtime panics are eaten by the runtime; the supervisor sees a happy `Ok(())` and the service is just… gone. There is no `JoinHandle` retention, no error channel, no `tracing::error!` on panic.

6. **Five out of five hot detached spawns are unsupervised (`RS-CR-01` / CC-01).** Across `rskit-http`, `rskit-grpc-server`, `rskit-discovery`, `rskit-mq` and `rskit-cache` we counted five `tokio::spawn(...)` invocations whose `JoinHandle` is dropped on the floor. Combined with the panic-on-unwrap pattern, **any one** of them silently turns the binary into a zombie that still answers `/healthz` (which is hard-coded to `Health::healthy`, see `RS-HI-37` / OB-07).

7. **Toolchain is incoherent.** `rust-toolchain.toml = "1.91"` (exact pin, `RS-HI-46` / TC-02) but `[workspace.package] rust-version = "1.85"` (`RS-HI-45` / AR-05 / TC-01). The CI matrix runs `[1.85, stable]` and `cargo +1.85 clippy` accepts code that `cargo +1.91 clippy` rejects (the 4 errors above only fire on 1.91). Whichever number is wrong, **both** numbers are wrong: an MSRV claim that the local toolchain disagrees with cannot be trusted by downstream consumers.

8. **Concurrency story is not actually tested.** Of 820 `#[tokio::test]` annotations across the workspace, **0** use `flavor = "multi_thread", worker_threads = N` (`RS-HI-31` / TS-02). All concurrency primitives — including the messaging `unsafe impl Send + Sync` in `rskit-messaging/src/middleware.rs:27-30` (`RS-HI-03` / CQ-03), the broadcast lag in `rskit-mq` (`RS-HI-09` / CC-04), and the `tokio::Mutex` in `rskit-cache::registry` (`RS-HI-07` / CC-02) — execute under the single-threaded current-thread runtime where data-race UB cannot be observed. Add `loom` zero, `proptest` zero, `cargo-fuzz` zero (`RS-HI-32` / TS-03).

9. **Bus factor 1, governance 0.** No `SECURITY.md` (`RS-CR-08` / RH-01), no `CODEOWNERS` (`RS-CR-11` / RH-02), no `MAINTAINERS` (`RS-HI-44` / RH-03), no `GOVERNANCE.md`, no `.editorconfig` (`RS-HI-43` / RH-04), no `.gitattributes` (`RS-HI-42` / RH-05), no pre-commit (`RS-HI-41` / RH-06), no Dependabot (`RS-HI-47` / TC-03), no SAST/CodeQL (`RS-HI-25` / CI-06), and 0/24 GitHub Actions are SHA-pinned — `dtolnay/rust-toolchain@master` and `actions/checkout@v4` (tag) are used directly (`RS-CR-05/06/07` / CI-01/02/03), which is exactly the supply-chain pattern that produced the `tj-actions/changed-files` compromise.

10. **Wins are real and worth keeping.** `subtle::ConstantTimeEq` for API-key compare (`rskit-auth/src/apikey.rs`), **rustls-only** across the workspace (no `native-tls`, no OpenSSL anywhere), `tracing` everywhere with **zero** `println!`/`eprintln!` in library code, JWT `Validation::new(algo)` correctly pins the alg per token (mitigates alg-confusion at the lib boundary), Argon2id as the only password hash, RFC-9457 `ProblemDetail` implemented bidirectionally with `tonic::Status`, `#[non_exhaustive]` on the public `ErrorCode` enum, single-source `[workspace.package]` + `[workspace.dependencies]`, two-version Rust matrix `[1.85, stable]`, `46/49` crates carry `[package.metadata.docs.rs]`, `32/49` crates have `tests/` directories, six `insta` golden suites, `deny.toml` with `unknown-registry = "deny"` + `unknown-git = "deny"`, and a substantive root `README.md` (richer than the gokit equivalent). The bones are good — the joints aren't tightened.

---

## §2. Findings Table

Severity counts: **Critical 11 · High 50 · Medium 50 · Low 25 · Nit 4** (140 total). Sorted by severity then category.

| ID | Sev | Category | Location | One-line title & recommendation | Effort | Maps-to |
|----|-----|----------|----------|---------------------------------|--------|---------|
| RS-CR-01 | Crit | Concurrency | `rskit-http/src/server.rs:88-102`, `rskit-grpc-server/src/server.rs:71-95`, `rskit-discovery/src/consul.rs:241-265`, `rskit-mq/src/broker.rs:188-210`, `rskit-cache/src/manager.rs:144-160` | Five detached `tokio::spawn(..)` sites drop the `JoinHandle`; panics/errors are swallowed → silent zombies. **Fix:** introduce `SupervisedTask` + `supervise()` helper, log `JoinError`, expose handle for graceful shutdown. | M | CC-01 |
| RS-CR-02 | Crit | Security/Auth | `rskit-auth/src/jwt.rs:140-198` | JWT RSA path constructs `DecodingKey::from_secret` for RS256/384/512 → silently runs HMAC over the PEM blob. **Fix:** typestate `JwtAlgo<Hs256>/<Rs256>` with sealed trait + alg-specific key constructors; delete the string-keyed enum. | L | SC-01 |
| RS-CR-03 | Crit | Security/Transport | `rskit-server/src/server.rs:142-178`, `rskit-server/src/config.rs:48-66` | `TlsConfig` parsed into `ServerConfig` but never wired into `tonic::transport::Server::builder()` → gRPC plaintext on the wire. Field flagged dead-code by nightly. **Fix:** require `ServerTls` typestate before `.serve()` returns; or wire `tls_config(..)` and gate by feature. | M | SC-02 |
| RS-CR-04 | Crit | Concurrency/HTTP | `rskit-http/src/server.rs:88-102` | `axum::serve(..).await.unwrap()` runs inside detached `tokio::spawn` → bind errors and runtime panics swallowed. **Fix:** typestate `HttpServer<Stopped>/<Bound>/<Running>`, return `Result` from `bind()`, retain JoinHandle. | M | SC-03 / CQ-02 |
| RS-CR-05 | Crit | CI/Supply-chain | `.github/workflows/*.yml` (24 actions) | 0/24 GitHub Actions are SHA-pinned. **Fix:** pin every `uses:` to a 40-char SHA + comment with version; add `pin-github-actions` pre-commit. | S | CI-01 |
| RS-CR-06 | Crit | CI/Supply-chain | `.github/workflows/ci.yml:32-38` | `dtolnay/rust-toolchain@master` — anyone with push to that repo can hijack our CI. **Fix:** pin to the action SHA of a specific tag (`@1.91.0`). | XS | CI-02 |
| RS-CR-07 | Crit | CI/Supply-chain | all workflows | No `permissions:` block at workflow level → workflows inherit broad `GITHUB_TOKEN`. **Fix:** add `permissions: contents: read` at workflow level; promote per-job. | XS | CI-03 |
| RS-CR-08 | Crit | Hygiene/Sec | repo root | No `SECURITY.md` → no documented vuln-disclosure channel. **Fix:** add `SECURITY.md` with PGP key + GitHub Private Vulnerability Reporting opt-in. | XS | RH-01 |
| RS-CR-09 | Crit | Release | `.github/workflows/` | No release pipeline whatsoever. **Fix:** adopt `release-plz` (Conventional Commits → CHANGELOG → tags → `cargo publish` topo-order); add cosign + SBOM + attest-build-provenance later. | M | RL-01 |
| RS-CR-10 | Crit | Release | `Cargo.toml` `[workspace.dependencies]` | All sibling-crate entries are `path = `-only; `cargo publish` will reject every crate that depends on another workspace member. **Fix:** every internal entry must be `{ path = "...", version = "=0.x.y" }`. | S | RL-02 |
| RS-CR-11 | Crit | Hygiene | repo root | No `CODEOWNERS` → no review enforcement, no auto-assign. **Fix:** add `.github/CODEOWNERS` with crate-folder ownership. | XS | RH-02 |

### High (50)

| ID | Sev | Category | Location | One-line title & recommendation | Effort | Maps-to |
|----|-----|----------|----------|---------------------------------|--------|---------|
| RS-HI-01 | High | Code Quality | workspace-wide | 714 `unwrap()`/`expect()` in non-test code. **Fix:** ban via `clippy::unwrap_used = "deny"` + `expect_used = "deny"` in `[workspace.lints]`; allow only in tests via `#[cfg_attr(test, allow(...))]`. | L | CQ-01 / SC-20 / TS-08 |
| RS-HI-02 | High | Code Quality | `rskit-http/src/server.rs:88-102` | `unwrap()` inside detached spawn — see RS-CR-04. **Fix:** propagate via channel + structured `tracing::error!`. | S | CQ-02 |
| RS-HI-03 | High | Code Quality / Concurrency | `rskit-messaging/src/middleware.rs:27-30` | `unsafe impl Send + Sync for MessageContext` over fields containing `Rc`/non-Send. **Fix:** delete `unsafe impl`; require `T: Send + Sync` bounds; if interior mutability needed, use `Arc<Mutex<T>>`. | S | CQ-03 / SC-13 |
| RS-HI-04 | High | Architecture | `rskit-di/src/registry.rs`, `rskit-component/src/registry.rs`, `rskit-config/src/registry.rs`, `rskit-messaging/src/registry.rs`, `rskit-llm/src/registry.rs` | Five divergent ad-hoc registries (`HashMap<TypeId, Box<dyn Any>>` variants). **Fix:** consolidate into `TypedRegistry<K, V>` in `rskit-core` with one well-tested impl. | L | AR-01 |
| RS-HI-05 | High | Architecture | `rskit-llm/Cargo.toml`, `rskit-llm-providers/Cargo.toml` | `rskit-llm ↔ rskit-llm-providers` cyclic via dev-deps + features. **Fix:** split provider trait into `rskit-llm-traits` (no deps); both crates depend on traits crate only. | M | AR-02 |
| RS-HI-06 | High | Concurrency | `rskit-cache/src/registry.rs:84-118` | `tokio::sync::Mutex` held across non-await section → use `parking_lot::Mutex` or `std::sync::Mutex`. | S | CC-02 |
| RS-HI-07 | High | Concurrency | `rskit-mq/src/select.rs:42-78` | `tokio::select!` without `biased;` → starvation under load + non-determinism in tests. **Fix:** add `biased;` and order branches by priority. | S | CC-03 |
| RS-HI-08 | High | Concurrency | `rskit-mq/src/broker.rs:188-210` | `broadcast::Receiver` `Lagged(n)` arm logs and continues silently. **Fix:** wrap in `LossyBroadcast` that increments `dropped_total` counter + emits `tracing::warn!`. | S | CC-04 |
| RS-HI-09 | High | Security | `rskit-auth/src/lib.rs` | Docs claim "OIDC supported" but no OIDC code path exists; only static JWT validation. **Fix:** implement via `openidconnect` crate or remove the claim from README. | M | SC-04 |
| RS-HI-10 | High | Security | `rskit-auth/src/apikey.rs:54-92` | API-key middleware: missing keys yield 401 with no `WWW-Authenticate` header; on bypass paths the middleware short-circuits to 200 even when key invalid. **Fix:** always set `WWW-Authenticate: Bearer realm=...`; reject on invalid before bypass check. | S | SC-05 |
| RS-HI-11 | High | Security/Obs | `rskit-logging/src/init.rs:38-72` | `RUST_LOG` silently overrides config-file level + masking is post-format (PII already in serialized JSON). **Fix:** treat config as source-of-truth; merge env via explicit precedence; mask in `Visit` not in formatter. | M | SC-06 / OB-04 |
| RS-HI-12 | High | Security | `Cargo.toml`, `rskit-auth/src/lib.rs` | `zeroize` declared as dep but never `Zeroize`/`ZeroizeOnDrop` derived for `Secret`/`ApiKey`/`JwtSecret`. **Fix:** wrap secret strings in `secrecy::SecretString`; derive `ZeroizeOnDrop`. | S | SC-07 |
| RS-HI-13 | High | Security | `rskit-encryption/src/aes.rs:44-78` | `AesGcm::new(secret: &str)` does `Sha256::digest(secret.as_bytes())` to "stretch" arbitrary input → low-entropy passphrases produce predictable keys. **Fix:** require `[u8; 32]` only; reject `&str`; if KDF needed, force Argon2id with explicit salt. | S | SC-08 |
| RS-HI-14 | High | Errors | `rskit-error/src/lib.rs:188-220` | `AppError::wrap(err)` matches a single root cause then funnels everything else to 500. **Fix:** replace with extensible classifier closures `Fn(&dyn Error) -> Option<ErrorCode>` registered per-crate. | M | ER-01 |
| RS-HI-15 | High | Errors | `rskit-error/src/lib.rs` | `AppError` is `!Clone` because of `Box<dyn Error + Send + Sync>` source → cannot be returned twice from a cached layer or compared in tests. **Fix:** store `Arc<dyn Error + Send + Sync>` instead of `Box`. | S | ER-02 |
| RS-HI-16 | High | Observability | workspace-wide; no `/healthz` route | No built-in `/livez` / `/readyz` from `rskit-server` or `rskit-http`. **Fix:** add `HealthRegistry` with pluggable `Probe` trait; mount `/livez` + `/readyz` by default. | M | OB-01 / SC-10 |
| RS-HI-17 | High | Observability | `rskit-tracing/src/init.rs:62-94` | `init_tracer()` does not set a global `TextMapPropagator` and there is no `Telemetry` façade for idempotent init/shutdown. **Fix:** introduce `Telemetry::init()` / `Telemetry::shutdown()` (idempotent), set `TraceContextPropagator` + `BaggagePropagator`. | M | OB-02 |
| RS-HI-18 | High | Observability/Sec | `rskit-http/src/middleware/trace.rs:24-58` | `TraceLayer` includes raw query string in spans → leaks PII (tokens, emails). **Fix:** strip query in `make_span_with`; allow-list specific keys via config. | S | OB-03 |
| RS-HI-19 | High | Testing | `.github/workflows/ci.yml` | No coverage gate; no `cargo-llvm-cov` job; no Codecov. **Fix:** add `cargo llvm-cov --workspace --fail-under-lines 70` job + Codecov upload. | S | TS-01 / CI-09 |
| RS-HI-20 | High | Testing | 820 `#[tokio::test]` annotations | 0/820 use `flavor = "multi_thread"`. **Fix:** codemod to add `#[tokio::test(flavor = "multi_thread", worker_threads = 4)]` for any test that exercises spawn/select/broadcast. | M | TS-02 |
| RS-HI-21 | High | Testing | workspace-wide | No `cargo-fuzz`, no `proptest`, no `loom`. **Fix:** add `fuzz/` skeleton for `rskit-http` body parsing + `rskit-auth` token parsing; add `loom` to `rskit-mq`. | L | TS-03 |
| RS-HI-22 | High | Testing | various | Single `#[ignore]` stub test as the entire integration plan. **Fix:** delete or implement; add `--include-ignored` to CI nightly job. | XS | TS-04 |
| RS-HI-23 | High | Testing | workspace-wide | No `Clock` abstraction → tests sleep on real wall-clock. **Fix:** introduce `trait Clock` + `MockClock`; inject via constructor. | M | TS-05 |
| RS-HI-24 | High | Performance | workspace-wide | 2/49 crates have any benchmarks at all. **Fix:** add Criterion benches to `rskit-http`, `rskit-grpc-server`, `rskit-cache`, `rskit-mq`, `rskit-auth`, `rskit-error`, `rskit-encryption`. | L | PF-01 |
| RS-HI-25 | High | Performance | `.github/workflows/ci.yml` | No bench-regression gate. **Fix:** add `bencher.dev` or `cargo-criterion --message-format=json` + `benchstat`-style threshold. | M | PF-03 |
| RS-HI-26 | High | Performance | **`rskit-ratelimit/src/limiter.rs:359`** | `unsafe { &*(self as *const _ as *const RateLimiter<O>) }` cast between distinct generic instantiations → **UB**. **Fix:** redesign as `RateLimiter<dyn Object>` or split into separate impls. | S | PF-08 / SC-12 |
| RS-HI-27 | High | Lint | `Cargo.toml` workspace root | No `[workspace.lints]` block. **Fix:** add the full block (see §6). | S | LT-01 |
| RS-HI-28 | High | Lint | `rust-toolchain.toml` vs CI | Toolchain drift: local `1.91`, CI runs clippy on `1.85`. **Fix:** unify on either (a) MSRV `1.85` and run clippy on both 1.85 + stable, or (b) MSRV `1.91` and bump rust-toolchain. | S | LT-02 / TC-01 |
| RS-HI-29 | High | Lint | workspace-wide | `cargo fmt --all -- --check` fails 23 hunks. **Fix:** run `cargo fmt --all` and add fmt-check to required CI status. | XS | LT-03 |
| RS-HI-30 | High | Lint | `Cargo.toml` workspace | Critical clippy lints not enabled (`unwrap_used`, `expect_used`, `panic`, `todo`, `dbg_macro`, `unimplemented`, `mem_forget`, `lossy_float_literal`, `let_underscore_must_use`, `wildcard_dependencies`). **Fix:** enable in `[workspace.lints.clippy]`. | S | LT-04 |
| RS-HI-31 | High | CI | `.github/workflows/ci.yml` | Matrix is `[ubuntu-latest, macos-latest]` only — no Windows, no aarch64-linux. **Fix:** add `ubuntu-24.04-arm` + `windows-latest`; gate platform-specific tests behind `#[cfg]`. | M | CI-07 |
| RS-HI-32 | High | CI | `.github/workflows/ci.yml` | Tests run via `cargo test` not `cargo nextest`. **Fix:** install `nextest`, run `cargo nextest run --profile ci --no-fail-fast`. | XS | CI-08 |
| RS-HI-33 | High | CI | `.github/workflows/ci.yml` | No `concurrency:` block → duplicate runs on rebase / push. **Fix:** add `concurrency: group: ci-${{ github.ref }}, cancel-in-progress: true`. | XS | CI-04 |
| RS-HI-34 | High | CI | `.github/workflows/` | No release workflow. **Fix:** see RS-CR-09. | M | CI-05 |
| RS-HI-35 | High | CI | `.github/workflows/` | No CodeQL / no SAST. **Fix:** add `.github/workflows/codeql.yml` for Rust + Actions analysis. | S | CI-06 |
| RS-HI-36 | High | Toolchain | `rust-toolchain.toml = "1.91"` | Exact pin (not `>=1.85`) blocks downstream consumers. **Fix:** unify with MSRV; pin only in CI matrix entries. | XS | TC-02 |
| RS-HI-37 | High | Toolchain | `.github/dependabot.yml` | File missing entirely. **Fix:** add Dependabot for `cargo`, `github-actions`, `docker`; group patch + minor; cooldown 7d. | XS | TC-03 |
| RS-HI-38 | High | Toolchain | `.github/workflows/ci.yml` | `cargo build` without `--locked` → CI silently regenerates `Cargo.lock`. **Fix:** `cargo build --locked --all-features --workspace`. | XS | TC-04 |
| RS-HI-39 | High | Docs | `docs/` | No `docs/adr/` directory; no architectural decision records. **Fix:** seed `docs/adr/0001-record-architecture-decisions.md` (Nygard MADR template). | S | DC-01 |
| RS-HI-40 | High | Docs | `MEDIA_IMPLEMENTATION.md` (root, 2463 lines) | A 2.5k-line implementation plan in repo root pollutes README context. **Fix:** move to `docs/design/media.md`; link from README. | XS | DC-02 |
| RS-HI-41 | High | Docs | 19/49 crates | 19 crates have only a skeletal `//! TODO` line in `lib.rs`. **Fix:** require minimum 5-line crate doc with example; gate via `#![warn(missing_docs)]`. | M | DC-03 |
| RS-HI-42 | High | Docs | 20/49 crates | 20 crates lack `#![warn(missing_docs)]` (or it's not workspace-default). **Fix:** add to `[workspace.lints.rust]`. | XS | DC-04 |
| RS-HI-43 | High | Hygiene | repo root | No `SECURITY.md`, `MAINTAINERS`, `GOVERNANCE.md`. **Fix:** see RS-CR-08, RS-HI-44, add governance doc with role definitions. | S | DC-05 |
| RS-HI-44 | High | Hygiene | repo root | No `MAINTAINERS` file. **Fix:** add list with handles + scope; cross-link from CODEOWNERS. | XS | RH-03 |
| RS-HI-45 | High | Hygiene | repo root | No `.editorconfig`. **Fix:** add standard Rust `.editorconfig` (4-space, LF, trim trailing). | XS | RH-04 |
| RS-HI-46 | High | Hygiene | repo root | No `.gitattributes`. **Fix:** add normalisation rules + `*.rs text eol=lf` + binary markers. | XS | RH-05 |
| RS-HI-47 | High | Hygiene | repo root | No `pre-commit` config. **Fix:** add `.pre-commit-config.yaml` with `cargo fmt`, `cargo clippy --fix`, `actionlint`, `zizmor`. | XS | RH-06 |
| RS-HI-48 | High | Hygiene | GitHub repo | Branch protection unverified (cannot check from clone). **Fix:** require PR reviews + CI green + signed commits on `main`; document in CONTRIBUTING. | XS | RH-07 |
| RS-HI-49 | High | Release | repo tags | 0 tags / 0 GitHub Releases / no SemVer policy doc. **Fix:** establish lockstep `0.1.0` baseline; add `docs/SEMVER.md`; sign tags with sigstore/cosign. | M | RL-03 |
| RS-HI-50 | High | Release | workspace | No `cargo-public-api` baselines for any crate. **Fix:** generate baselines, commit to `crates/*/public-api/`, run `cargo public-api --diff-git-checkouts` in CI. | M | RL-04 |

### Medium (50)

| ID | Sev | Category | Location | One-line title & recommendation | Maps-to |
|----|-----|----------|----------|---------------------------------|---------|
| RS-ME-01 | Med | Code Quality | `rskit-retry/src/lib.rs:62-78` | `RetryConfig::with_max_attempts(0)` panics at runtime. **Fix:** `NonZeroU32` typestate. | CQ-04 |
| RS-ME-02 | Med | Code Quality | many public enums | Public enums lack `#[non_exhaustive]` → adding a variant is a breaking change. **Fix:** add to all pub enums except `ErrorCode`. | CQ-05 |
| RS-ME-03 | Med | Code Quality | `clippy.toml` | File is empty / placeholder. **Fix:** populate with `disallowed-methods`, `disallowed-types`, `cognitive-complexity-threshold`, `too-many-arguments-threshold`. | CQ-06 |
| RS-ME-04 | Med | Architecture | `rskit-component/src/lazy.rs:88-122` | `LazyComponent` factory wraps init in `tokio::sync::Mutex` → first call serializes. **Fix:** use `tokio::sync::OnceCell::get_or_try_init`. | AR-03 |
| RS-ME-05 | Med | Architecture | `rskit-di/src/container.rs:140-178` | `OnceLock<...>` per-key duplicates initialization work across keys. **Fix:** route through one `TypedRegistry` with deferred init futures. | AR-04 |
| RS-ME-06 | Med | Architecture | `Cargo.toml` rust-version | MSRV 1.85 vs toolchain 1.91 mismatch (also tracked as RS-HI-28). | AR-05 |
| RS-ME-07 | Med | Architecture | `deny.toml` | Permissive: `multiple-versions = "warn"`, `wildcards = "allow"`, `git-allow = ["github.com"]`. **Fix:** tighten to `deny`/`deny`/explicit list. | AR-06 / LT-08 / LT-09 |
| RS-ME-08 | Med | Concurrency | `rskit-server/src/registration.rs:92-118` | `tokio::task::block_in_place` for sync registration call inside an `async fn`. **Fix:** use `spawn_blocking` and `.await` the JoinHandle, or convert call to async. | CC-05 |
| RS-ME-09 | Med | Concurrency | tests | `multi_thread` flavor missing on every `#[tokio::test]` — see RS-HI-20. | CC-06 |
| RS-ME-10 | Med | Concurrency | `rskit-ratelimit/src/limiter.rs:228-260` | `Drop` impl spawns shutdown task without `await` → leak under heavy churn. **Fix:** explicit `async fn shutdown(self)` + `Drop` warns if not called. | CC-07 |
| RS-ME-11 | Med | Security | `rskit-http/src/cors.rs:18-54` | CORS layer is built but only wired in one example; not the default. **Fix:** make `CorsLayer` the default in `HttpServerBuilder`; document explicit opt-out. | SC-09 |
| RS-ME-12 | Med | Security | `rskit-http/src/server.rs` | No built-in `/healthz` (also RS-HI-16). | SC-10 |
| RS-ME-13 | Med | Security | `rskit-uri/src/lib.rs:38-56` | `OnceLock<String>` for global `type_base_uri` → process-global mutability. **Fix:** require explicit context arg or builder injection. | SC-11 |
| RS-ME-14 | Med | Security | `rskit-ratelimit/src/limiter.rs:359` | UB cast — see RS-HI-26. | SC-12 |
| RS-ME-15 | Med | Security | `rskit-messaging/src/middleware.rs:27-30` | Unsound `unsafe impl Send/Sync` — see RS-HI-03. | SC-13 |
| RS-ME-16 | Med | Security | `rskit-httpclient/src/client.rs:42-78` | No TLS knobs surfaced (cipher list, min version, custom roots). **Fix:** expose `ClientTlsConfig` builder; default to `TLS1.3` + system roots + `webpki-roots`. | SC-14 |
| RS-ME-17 | Med | CI | `.github/workflows/ci.yml` | Several actions used by `@v4` tag instead of SHA — see RS-CR-05. | SC-15 |
| RS-ME-18 | Med | CI | toolchain mismatch — see RS-HI-28. | SC-16 |
| RS-ME-19 | Med | CI | `.github/workflows/` | No SBOM, no cosign, no SLSA, no CodeQL — see RS-HI-35 + release plan. | SC-17 |
| RS-ME-20 | Med | Errors | `rskit-error/src/lib.rs` | No per-crate sentinel error types. **Fix:** introduce `pub enum AuthError`, `pub enum HttpError`, etc.; convert into `AppError` at boundary. | ER-03 |
| RS-ME-21 | Med | Errors | `rskit-error/src/lib.rs:240-272` | Allocation on every error message via `format!`. **Fix:** lazy `Cow<'static, str>` or `&'static str` for static cases. | ER-04 |
| RS-ME-22 | Med | Errors | `rskit-error/src/from.rs:18-32` | `From<serde_json::Error> for AppError` always maps to 422 even for I/O errors during deserialize from a stream. **Fix:** match on `error.classify()`. | ER-05 |
| RS-ME-23 | Med | Observability | logging — see RS-HI-11 (post-format masking). | OB-04 |
| RS-ME-24 | Med | Observability | `rskit-tracing/src/init.rs:42-64` | `Resource` missing `service.version`, `service.instance.id`, `deployment.environment`. **Fix:** populate from env + `option_env!("CARGO_PKG_VERSION")`. | OB-05 |
| RS-ME-25 | Med | Observability | `rskit-http` | No built-in HTTP RED (Rate/Errors/Duration) metrics layer. **Fix:** add `MetricsLayer` using `metrics` crate w/ Prometheus exporter wired via feature. | OB-06 |
| RS-ME-26 | Med | Observability | `rskit-server/src/health.rs:28-44` | `Health::healthy` is hard-coded `Ok(true)` — see RS-HI-16. | OB-07 |
| RS-ME-27 | Med | Testing | dim3 §1 | Test split between `tests/` dirs and inline `#[cfg(test)]` is opaque. **Fix:** convention doc; prefer `tests/` for integration, inline for unit. | TS-06 |
| RS-ME-28 | Med | Performance | `rskit-bench/` | Crate is named `rskit-bench` but contains ML model evaluation, not perf benchmarks. **Fix:** rename to `rskit-evals`; create new `benches/` per crate. | PF-02 |
| RS-ME-29 | Med | Performance | many traits | `Pin<Box<dyn Future>>` returns from sync trait fns → heap alloc per call. **Fix:** prefer `async fn` in trait (1.75+) or `impl Future` GAT. | PF-04 |
| RS-ME-30 | Med | Performance | hot paths | Heavy `.clone()` on `String`/`Vec<u8>` in middleware chains. **Fix:** prefer `Bytes` + `Arc<str>` slices. | PF-05 |
| RS-ME-31 | Med | Performance | workspace-wide | No `pprof`, no `tokio-console`, no `flamegraph` integration in dev profile. **Fix:** add feature `pprof`, document `cargo flamegraph` recipe. | PF-06 |
| RS-ME-32 | Med | Performance | `rskit-cache`, `rskit-mq` | No object pooling for hot allocations. **Fix:** evaluate `bytes::BytesMut::reserve` reuse + `object-pool` crate. | PF-07 |
| RS-ME-33 | Med | Lint | CI | `cargo clippy` invoked without `--all-features`. **Fix:** run `cargo hack clippy --feature-powerset --depth 2`. | LT-06 |
| RS-ME-34 | Med | Lint | repo root | `cargo machete` / `cargo udeps` not run in CI. **Fix:** add `machete` job (gates merges on unused deps). | LT-05 |
| RS-ME-35 | Med | CI | `.github/workflows/ci.yml` | clippy job is Linux-only. **Fix:** matrix clippy across OSes (cheap; finds windows path-sep bugs). | CI-10 |
| RS-ME-36 | Med | CI | various jobs | No `--locked`. (Also see RS-HI-38.) | CI-11 |
| RS-ME-37 | Med | CI | feature sweep | No `cargo hack --feature-powerset` job. **Fix:** add nightly cron. | CI-12 |
| RS-ME-38 | Med | CI | none | No `cargo semver-checks` job. **Fix:** add release-gate job. | CI-13 |
| RS-ME-39 | Med | CI | none | No `cargo public-api` job. **Fix:** add as required check on release PRs. | CI-14 |
| RS-ME-40 | Med | CI | docs | No `cargo doc --all-features --no-deps --document-private-items` job — broken intra-doc links can land on main. **Fix:** add docs job with `RUSTDOCFLAGS="-D warnings"`. | CI-15 |
| RS-ME-41 | Med | CI | `rust-cache` not configured | Build cache absent → CI is slow + flaky. **Fix:** `Swatinem/rust-cache@<sha>` step. | CI-16 |
| RS-ME-42 | Med | CI | nextest archives | Test artefacts not uploaded on failure. **Fix:** `actions/upload-artifact` for `target/nextest/ci/test.results.xml`. | CI-17 |
| RS-ME-43 | Med | CI | `.github/workflows/` | `audit` job runs on push but not on schedule. **Fix:** add cron `0 6 * * *` audit-only workflow. | CI-18 |
| RS-ME-44 | Med | CI | `.github/workflows/` | `actionlint` + `zizmor` not in CI. **Fix:** add `actionlint` + `zizmor` jobs. | CI-19 |
| RS-ME-45 | Med | CI | dependency review | No `dependency-review-action` on PRs. **Fix:** add `actions/dependency-review-action` w/ deny licences. | CI-20 |
| RS-ME-46 | Med | CI | `.github/workflows/ci.yml` | No required-check aggregation job. **Fix:** add `ci-status` job with `needs:` for every required check; mark only that as branch-protected. | CI-21 |
| RS-ME-47 | Med | Toolchain | `rust-toolchain.toml` | `components = ["rustfmt", "clippy"]` only — no `rust-src`/`rust-analyzer` for contributors. **Fix:** add components or document local install. | TC-05 |
| RS-ME-48 | Med | Toolchain | various | Mixed use of `cargo` plugins not documented (`cargo-deny`, `cargo-audit`, `cargo-nextest`, `cargo-llvm-cov`, `cargo-public-api`, `cargo-semver-checks`, `cargo-hack`, `cargo-msrv`). **Fix:** `tools.toml` or `cargo-binstall` script. | TC-06 |
| RS-ME-49 | Med | Docs | various | README sub-sections per crate are inconsistent — some have examples, some don't. **Fix:** standard README template; doc-test the example block. | DC-06 |
| RS-ME-50 | Med | Hygiene | `.gitignore` | Missing `*.profraw`, `tarpaulin-report.html`, `lcov.info`, `flamegraph.svg`, `perf.data*`. **Fix:** extend ignore list. | RH-08 |

### Low (25)

| ID | Sev | Category | Location | One-line title & recommendation | Maps-to |
|----|-----|----------|----------|---------------------------------|---------|
| RS-LO-01 | Low | Code Quality | many | Excessive `Box<dyn Trait>` where generics suffice. **Fix:** prefer `impl Trait` for hot paths. | CQ-07 |
| RS-LO-02 | Low | Code Quality | `rskit-error/src/lib.rs` | `pub` fields on `AppError` allow callers to mutate state. **Fix:** seal with constructors + getters. | CQ-08 |
| RS-LO-03 | Low | Errors | wire layer | Cause chain dropped on serialization. **Fix:** RFC-9457 `extensions` map can carry `cause: [str]` opt-in. | ER-06 |
| RS-LO-04 | Low | Errors | docs | `Box<dyn Error>` discipline undocumented. **Fix:** add `docs/error-handling.md`. | ER-07 |
| RS-LO-05 | Low | Observability | `rskit-tracing/src/shutdown.rs:14-30` | Sync `shutdown()` in `Drop`. **Fix:** require explicit `Telemetry::shutdown().await`. | OB-08 |
| RS-LO-06 | Low | Observability | `rskit-server/src/health.rs` | `ComponentHealth` missing `checked_at` + `latency`. **Fix:** add fields; default-impl in trait. | OB-09 |
| RS-LO-07 | Low | Concurrency | `rskit-config/src/global.rs:18-32` | `RwLock<Option<String>>` global state. **Fix:** thread context explicitly. | CC-08 |
| RS-LO-08 | Low | Concurrency | various | Fixed worker count instead of `available_parallelism()`. **Fix:** use `std::thread::available_parallelism().map(NonZeroUsize::get).unwrap_or(4)`. | CC-09 |
| RS-LO-09 | Low | Security | `Cargo.toml` | `cargo-audit` and `cargo-deny advisories` are duplicates. **Fix:** keep deny only; add `cargo-osv-scanner` if extra coverage wanted. | SC-18 / LT-09 |
| RS-LO-10 | Low | Security | none | No fuzz harnesses (also RS-HI-21). | SC-19 |
| RS-LO-11 | Low | Lint | `clippy.toml` | Empty (also RS-ME-03). | LT-07 |
| RS-LO-12 | Low | Lint | `deny.toml` | `multiple-versions = "warn"` only (also RS-ME-07). | LT-08 |
| RS-LO-13 | Low | CI | `.github/workflows/` | `cargo-deny` job runs on every push but not for licence delta only on PRs. **Fix:** keep both, but split fast/slow jobs. | CI-22 |
| RS-LO-14 | Low | Toolchain | `rust-toolchain.toml` | No `profile = "minimal"` → CI installs full profile. **Fix:** set `profile = "minimal"` + components only. | TC-07 |
| RS-LO-15 | Low | Toolchain | `Cargo.toml` | `[profile.release-lto]` not defined. **Fix:** add `[profile.release-lto]` inheriting release + `lto = "fat"`. | TC-08 |
| RS-LO-16 | Low | Toolchain | none | No `cargo-msrv` job verifying claimed MSRV. **Fix:** add `cargo-msrv verify` to CI. | TC-09 |
| RS-LO-17 | Low | Toolchain | none | `Cargo.lock` checked in for libraries (debatable). Document policy. | TC-10 |
| RS-LO-18 | Low | Toolchain | none | No `rust-version` in per-crate `Cargo.toml` — relies on workspace. Document. | TC-11 |
| RS-LO-19 | Low | Toolchain | none | No `[patch.crates-io]` discipline doc. **Fix:** policy in CONTRIBUTING. | TC-12 |
| RS-LO-20 | Low | Toolchain | none | No `cargo-vet` for transitive review. **Fix:** evaluate. | TC-13 |
| RS-LO-21 | Low | Docs | various | Many `lib.rs` lack examples in doc comments → `cargo test --doc` is ~empty. **Fix:** add doctest per public API. | DC-07 |
| RS-LO-22 | Low | Docs | none | No `CONTRIBUTING.md` with crate-by-crate runbook. **Fix:** add. | DC-08 |
| RS-LO-23 | Low | Docs | none | No `CHANGELOG.md` per-crate; only root. **Fix:** generate via `release-plz`. | DC-09 / RL-05 |
| RS-LO-24 | Low | Hygiene | `.github/` | No issue templates / PR template. **Fix:** add `.github/ISSUE_TEMPLATE/{bug,feature}.yml` + `PULL_REQUEST_TEMPLATE.md`. | RH-09 |
| RS-LO-25 | Low | Hygiene | `.github/` | No `FUNDING.yml`. **Fix:** if applicable. | RH-10 |

### Nit (4)

| ID | Sev | Category | Location | One-line title | Maps-to |
|----|-----|----------|----------|----------------|---------|
| RS-NI-01 | Nit | Code Quality | `rskit-cli/src/main.rs:24` | `unsafe { std::env::set_var(..) }` for setting log level — avoid; use `tracing_subscriber::EnvFilter::try_from_default_env().or_else(..)`. | CQ-09 |
| RS-NI-02 | Nit | Architecture | `rskit-dataset/Cargo.toml`, `rskit-cli/Cargo.toml` | Inverted feature: `dataset/cli` enabled by default; should be opt-in. | AR-07 |
| RS-NI-03 | Nit | Architecture | `rskit-integration` | Crate is undocumented (no `lib.rs` doc, no README). | AR-08 |
| RS-NI-04 | Nit | Hygiene | various | Some commits not signed. **Fix:** require signed commits via branch protection. | RH-11 / RH-12 / RH-13 |


---

## §2.2 Per-Critical / High inline detail

### RS-CR-01 — Five unsupervised detached `tokio::spawn`s
**Evidence (verbatim):** `rskit-http/src/server.rs:88-102` spawns the axum serve future and immediately returns; `rskit-grpc-server/src/server.rs:71-95` same pattern with `tonic::transport::Server::serve_with_shutdown`; `rskit-discovery/src/consul.rs:241-265` spawns the heartbeat loop; `rskit-mq/src/broker.rs:188-210` spawns the per-topic dispatcher; `rskit-cache/src/manager.rs:144-160` spawns the eviction sweeper.
**Impact:** Any panic — including the `unwrap()` immediately following each `axum::serve(..).await` — vanishes into the runtime. `/healthz` returns 200 (it's hard-coded, OB-07/RS-ME-26) while the listener is dead. This is the single most likely "ghost service" cause in production.
**Recommendation:** Introduce `rskit-core::SupervisedTask` + `supervise(future, name)`; retain `JoinHandle`; on `JoinError::is_panic()` log structured + restart with bounded backoff or signal shutdown via `CancellationToken`.
**Effort:** M (one helper crate-wide + per-call site refactor; ~half day)
**Dependencies:** RS-CR-04 (HttpServer typestate), RS-HI-01 (no-unwrap lint).

### RS-CR-02 — JWT RSA path silently runs HMAC
**Evidence (verbatim):** `rskit-auth/src/jwt.rs:140-198`: every match arm constructs `DecodingKey::from_secret(secret.as_bytes())` regardless of `Algorithm::HS256 | RS256 | RS384 | RS512`. The advertised RSA support is dead code.
**Impact:** A deployment that configures `[auth.jwt] algo = "rs256"` and supplies a public key PEM gets an HMAC validation against the PEM bytes — trivially forgeable by anyone holding the public key (which by definition is public).
**Recommendation:** Replace `JwtAlgo` enum with sealed-trait typestate (see §4.A); separate `Hs256Key(SecretString)` and `Rs256Key(DecodingKey)` constructors; the validator becomes `Validator<A: JwtAlgo>` so the compiler enforces key/alg pairing.
**Effort:** L (≈1 day; touches public API of `rskit-auth`, requires migration note)
**Dependencies:** RS-HI-09 (OIDC), RS-HI-10 (WWW-Authenticate).

### RS-CR-03 — gRPC plaintext despite `TlsConfig` field
**Evidence (verbatim):** `rskit-server/src/server.rs:142-178` builds the `Server::builder()` chain but never calls `.tls_config(server_tls)`. `rskit-server/src/config.rs:48-66` defines `pub struct TlsConfig { pub cert_pem: PathBuf, pub key_pem: PathBuf, pub client_ca_pem: Option<PathBuf> }` — only ever read for `Debug` formatting. Nightly `cargo +nightly check --features tls` flags `TlsConfig` dead-code.
**Impact:** Any user with `[server.tls]` populated in TOML believes traffic is mTLS-protected. It is not.
**Recommendation:** Adopt `Server<NoTls> -> Server<Tls>` typestate so `.serve()` is unreachable on `NoTls` unless the user explicitly opts into plaintext via `.allow_plaintext()`. As a quick fix, wire `tls_config` and emit `tracing::warn!("TLS configured but feature `tls` not enabled")` if the feature is off.
**Effort:** M
**Dependencies:** RS-CR-02 wiring of WWW-Authenticate.

### RS-CR-04 — HttpServer panics inside detached spawn
**Evidence (verbatim):** `rskit-http/src/server.rs:88-102`:
```rust
tokio::spawn(async move {
    axum::serve(listener, app).await.unwrap();
});
Ok(())
```
**Impact:** Bind failures (port already in use, EACCES on <1024) and any axum runtime panic disappear into the runtime. The function returns `Ok(())` so the supervisor never knows.
**Recommendation:** Typestate `HttpServer<Stopped>/<Bound>/<Running>`; `bind()` returns `Result<HttpServer<Bound>, BindError>`; `start()` retains `JoinHandle<Result<(), serve::Error>>` and exposes `wait()` / `shutdown()`.
**Effort:** M
**Dependencies:** RS-CR-01.

### RS-CR-05/06/07 — Supply-chain hygiene gaps in CI
- **CR-05** 0/24 actions SHA-pinned. Pin every `uses:` to the 40-char SHA matching the desired tag and append the human-readable version as a comment (e.g. `uses: actions/checkout@<sha>  # v4.2.2`).
- **CR-06** `dtolnay/rust-toolchain@master` is the canonical example of "do not do this." Anyone with push to `dtolnay/rust-toolchain` runs arbitrary code in our build. Pin to a specific commit SHA matching `@1.91.0`.
- **CR-07** Add `permissions: contents: read` at workflow root; promote per-job for `id-token: write` (cosign), `pull-requests: write` (release-plz), `issues: write` (dependabot auto-merge).

### RS-CR-08 / RS-CR-11 — `SECURITY.md` + `CODEOWNERS`
Add `SECURITY.md` documenting (a) supported versions table, (b) how to report (PGP key fingerprint + GitHub Private Vulnerability Reporting URL), (c) coordinated-disclosure SLA. Add `.github/CODEOWNERS` mapping `crates/rskit-auth/ @rskit/sec-reviewers` etc. Without these two files, the project cannot accept a CVE assignment and PRs that touch security code can land without a security review.

### RS-CR-09 — No release pipeline
There is no `release.yml`, no `cargo publish` automation, no signed tags. **Recommendation:** adopt `release-plz` driven by Conventional Commits; one workflow runs on push to `main`, opens a release-PR with version bumps + CHANGELOG, and on merge tags + publishes in topo order. Phase 2: add `cosign sign-blob` for tags, `cargo cyclonedx` for SBOM, `actions/attest-build-provenance` for SLSA L3.

### RS-CR-10 — `cargo publish` will reject path-only deps
Every entry in `[workspace.dependencies]` for an internal crate currently looks like:
```toml
rskit-core = { path = "rs-services/rskit-core" }
```
`cargo publish` requires either `version =` or `registry =` for each dependency. Fix every entry to:
```toml
rskit-core = { path = "rs-services/rskit-core", version = "=0.1.0" }
```
Verify with `cargo publish -p rskit-core --dry-run` and iterate up the dependency DAG.

### High details (selected; full list in §2 table)

**RS-HI-01 (714 unwrap/expect).** Treat as a workspace-wide lint debt. Enable `clippy::unwrap_used = "deny"` + `clippy::expect_used = "deny"` in `[workspace.lints.clippy]`; allow per-line in tests with `#[cfg_attr(test, allow(clippy::unwrap_used))]`. Burn down in PRs grouped by crate; estimate 2–3 days for the long tail.

**RS-HI-03 (unsound `unsafe impl Send + Sync`).** `rskit-messaging/src/middleware.rs:27-30` declares `unsafe impl<T> Send for MessageContext<T> {}` without bounds on `T`. If `T` is `Rc<U>` (which downstream users may legitimately want for builder ergonomics), this is undefined behaviour the moment the context crosses an `await` point. **Delete** the unsafe impls and add `where T: Send + Sync` bounds; if interior mutability is required, wrap the field in `Arc<tokio::sync::Mutex<T>>`.

**RS-HI-04 (5 divergent registries).** `rskit-di`, `rskit-component`, `rskit-config`, `rskit-messaging`, `rskit-llm` each implement their own `HashMap<TypeId, Box<dyn Any>>`. Consolidate into a single `TypedRegistry<K: Hash + Eq, V: 'static>` in `rskit-core` with documented thread-safety. Migrate one registry at a time; the API surface is small (`get`, `insert`, `get_or_init`).

**RS-HI-05 (LLM cycle).** Break by extracting `rskit-llm-traits` (no deps); both `rskit-llm` and `rskit-llm-providers` depend on `rskit-llm-traits` only. The cycle currently relies on dev-deps + features which makes `cargo doc --all-features` non-deterministic.

**RS-HI-09 (no OIDC).** README claims OIDC support; code does only static JWT validation. Either implement (use `openidconnect` crate; ~1 week incl. discovery + JWKS rotation) or remove the claim.

**RS-HI-13 (AesGcm SHA-256-stretches weak keys).** `rskit-encryption/src/aes.rs:44-78` accepts `&str` and unconditionally hashes with SHA-256 to derive a key. A 4-character passphrase yields a perfectly valid 256-bit AES key — false sense of security. Reject `&str` at the type level: `AesGcm::new(key: &[u8; 32])`. If KDF is needed, force callers to use `Argon2id` with explicit `Salt`.

**RS-HI-16 / RS-ME-12 / RS-ME-26 (no health probes).** Provide `HealthRegistry` with `trait Probe { async fn check(&self) -> ProbeResult; }`; mount `/livez` (process alive) + `/readyz` (all probes ready) by default. Today the only "health" is a hard-coded `Ok(true)`.

**RS-HI-26 (UB cast in ratelimit).** `rskit-ratelimit/src/limiter.rs:359`:
```rust
let coerced: &RateLimiter<O> = unsafe { &*(self as *const _ as *const RateLimiter<O>) };
```
Casts between distinct generic instantiations — unconditional UB. The redesign is to make `RateLimiter` non-generic by storing `Box<dyn Object>` internally, or to split into separate `impl` blocks where the cast is unnecessary.

**RS-HI-49 (zero tags / no SemVer policy).** Cut a `0.1.0` baseline release once the Critical list is resolved; commit a `docs/SEMVER.md` documenting lockstep workspace versioning + breaking-change deprecation window (1 minor version with `#[deprecated]`).

---

## §3. 14-Dimension Assessment

For each dimension: **Good** (what's working), **Problems** (finding-IDs), **Redesign anchors**.

### 3.1 Code Quality
- **Good:** `tracing` everywhere; no `println!`/`eprintln!` in libraries; `#[non_exhaustive]` on `ErrorCode`; substantive crate organisation; consistent use of `thiserror`.
- **Problems:** RS-HI-01, RS-HI-02, RS-HI-03, RS-ME-01, RS-ME-02, RS-ME-03, RS-LO-01, RS-LO-02, RS-NI-01.
- **Redesign:** §4.A typestate JwtAlgo, §4.G `[workspace.lints]`, §4.H NonZeroU32 for retry config.

### 3.2 Architecture
- **Good:** Clean crate boundaries (49 crates, mostly single-responsibility); `[workspace.package]` + `[workspace.dependencies]` single source; `rskit-core` as foundational.
- **Problems:** RS-HI-04 (5 registries), RS-HI-05 (LLM cycle), RS-ME-04 (LazyComponent Mutex), RS-ME-05 (DI duplication), RS-ME-06 (MSRV mismatch), RS-ME-07 (deny.toml), RS-NI-02, RS-NI-03.
- **Redesign:** §4.C `TypedRegistry`, §4.D LLM split, §4.E async LazyComponent.

### 3.3 Concurrency
- **Good:** `tokio` discipline; rare use of unsafe synchronization; `CancellationToken` adopted in some modules.
- **Problems:** RS-CR-01, RS-CR-04, RS-HI-03, RS-HI-06, RS-HI-07, RS-HI-08, RS-ME-08, RS-ME-09, RS-ME-10, RS-LO-07, RS-LO-08.
- **Redesign:** §4.B `SupervisedTask` + `supervise()`, §4.F `LossyBroadcast`, §4.J typestate HttpServer.

### 3.4 Security
- **Good:** **rustls-only** (no `native-tls`/OpenSSL anywhere — major win); `subtle::ConstantTimeEq` for API-key compare; Argon2id only password hash; JWT `Validation::new(algo)` correctly pins per token; RFC-9457 `ProblemDetail` bidirectional with `tonic::Status`.
- **Problems:** RS-CR-02, RS-CR-03, RS-CR-05, RS-CR-06, RS-CR-07, RS-CR-08, RS-CR-11, RS-HI-09, RS-HI-10, RS-HI-11, RS-HI-12, RS-HI-13, RS-HI-26, RS-HI-35, RS-ME-11..19, RS-LO-09, RS-LO-10.
- **Redesign:** §4.A typestate JWT, §4.K `AuthMode` enum + `WWW-Authenticate`, §4.L `IntoResponse for AppError`, §4.O Telemetry façade w/ propagators, §4.M `HealthRegistry`.

### 3.5 Errors
- **Good:** `AppError::Display` does NOT leak the cause chain (improvement vs. gokit ER-05); `#[non_exhaustive]` on `ErrorCode`; RFC-9457 `ProblemDetail` representation.
- **Problems:** RS-HI-14, RS-HI-15, RS-ME-20, RS-ME-21, RS-ME-22, RS-LO-03, RS-LO-04.
- **Redesign:** §4.L `IntoResponse for AppError`, §4.N classifier `wrap()`, §4.P `Arc<dyn Error>` for Clone, per-crate sentinel errors.

### 3.6 Observability
- **Good:** `tracing` adoption; `[package.metadata.docs.rs]` configured for OTLP feature in 46/49 crates; `insta` golden tests for log format.
- **Problems:** RS-HI-16, RS-HI-17, RS-HI-18, RS-ME-23..26, RS-LO-05, RS-LO-06.
- **Redesign:** §4.M `HealthRegistry` (`/livez`+`/readyz`), §4.O idempotent `Telemetry::init/shutdown` with global propagator + masking-as-Layer (`MaskingLayer<S>`).

### 3.7 Testing
- **Good:** 2 470 tests pass; `insta` golden suites (6); 32/49 crates have `tests/`; `#[cfg(test)]` modules well-distributed; `mockito`/`wiremock` used in some HTTP tests.
- **Problems:** RS-HI-19, RS-HI-20, RS-HI-21, RS-HI-22, RS-HI-23, RS-ME-27.
- **Redesign:** §4.Q `Clock` trait + `MockClock`, §4.R cargo-fuzz skeleton, multi_thread codemod, `INSTA_UPDATE=no` in CI.

### 3.8 Performance
- **Good:** `Bytes` used in some hot paths; `tokio` runtime tuned.
- **Problems:** RS-HI-24, RS-HI-25, RS-HI-26 (UB), RS-ME-28..32.
- **Redesign:** §4.S Criterion bench skeleton + bench-gate; §4.T remove ratelimit unsafe cast; `pprof` feature; object pooling for cache + mq.

### 3.9 Lint
- **Good:** `clippy` runs in CI; `rustfmt` tooled.
- **Problems:** RS-HI-27, RS-HI-28, RS-HI-29, RS-HI-30, RS-ME-33, RS-ME-34, RS-LO-11, RS-LO-12.
- **Redesign:** §6 full `[workspace.lints]` + `clippy.toml` + `cargo machete` job.

### 3.10 CI
- **Good:** Two-version matrix `[1.85, stable]`; `cargo deny` job exists; `audit` job exists; tests run on push.
- **Problems:** RS-CR-05/06/07, RS-HI-31..35, RS-HI-38, RS-ME-35..46, RS-LO-13.
- **Redesign:** §5 CI YAML drop-ins (ci.yml, audit-cron.yml, codeql.yml, fuzz.yml, release.yml, dependabot.yml).

### 3.11 Toolchain
- **Good:** `rust-toolchain.toml` exists; `[workspace.package]` centralised.
- **Problems:** RS-HI-28, RS-HI-36, RS-HI-37, RS-HI-38, RS-ME-47, RS-ME-48, RS-LO-14..20.
- **Redesign:** Unify MSRV/channel; `dependabot.yml` w/ cooldown + groups; `cargo-binstall` script for plugins.

### 3.12 Docs
- **Good:** Substantive root README; 46/49 crates configure `[package.metadata.docs.rs]`; `CHANGELOG` follows Keep-a-Changelog at root.
- **Problems:** RS-HI-39, RS-HI-40, RS-HI-41, RS-HI-42, RS-ME-49, RS-LO-21..23.
- **Redesign:** `docs/adr/`, move `MEDIA_IMPLEMENTATION.md` → `docs/design/media.md`, gate `#![warn(missing_docs)]` workspace-wide.

### 3.13 Release
- **Good:** Keep-a-Changelog format adopted at root; `[workspace.package]` makes lockstep versioning trivial.
- **Problems:** RS-CR-09, RS-CR-10, RS-HI-49, RS-HI-50.
- **Redesign:** §5.E `release-plz` workflow; `release-plz.toml`; cosign + SBOM + SLSA in phase 2.

### 3.14 Hygiene
- **Good:** `deny.toml` with `unknown-registry = "deny"` + `unknown-git = "deny"`; LICENSE present (Apache-2.0/MIT dual).
- **Problems:** RS-CR-08, RS-CR-11, RS-HI-43..48, RS-ME-50, RS-LO-24, RS-LO-25, RS-NI-04.
- **Redesign:** Add `SECURITY.md`, `CODEOWNERS`, `MAINTAINERS`, `GOVERNANCE.md`, `.editorconfig`, `.gitattributes`, pre-commit, issue/PR templates.


---

## §4. Redesign Sketches

### 4.A Typestate `JwtAlgo` (RS-CR-02)
```rust
mod sealed { pub trait Sealed {} }
pub trait JwtAlgo: sealed::Sealed { const ALG: jsonwebtoken::Algorithm; type Key; }

pub struct Hs256; pub struct Rs256; pub struct Es256;
impl sealed::Sealed for Hs256 {} impl sealed::Sealed for Rs256 {} impl sealed::Sealed for Es256 {}

impl JwtAlgo for Hs256 { const ALG: jsonwebtoken::Algorithm = jsonwebtoken::Algorithm::HS256;
    type Key = secrecy::SecretBox<[u8]>; }
impl JwtAlgo for Rs256 { const ALG: jsonwebtoken::Algorithm = jsonwebtoken::Algorithm::RS256;
    type Key = jsonwebtoken::DecodingKey; }
impl JwtAlgo for Es256 { const ALG: jsonwebtoken::Algorithm = jsonwebtoken::Algorithm::ES256;
    type Key = jsonwebtoken::DecodingKey; }

pub struct Validator<A: JwtAlgo> { key: A::Key, validation: jsonwebtoken::Validation }

impl Validator<Hs256> {
    pub fn new(secret: secrecy::SecretBox<[u8]>) -> Self { /* HS-only */ }
}
impl Validator<Rs256> {
    pub fn from_pem_public(pem: &[u8]) -> Result<Self, JwtError> {
        Ok(Self { key: jsonwebtoken::DecodingKey::from_rsa_pem(pem)?,
                  validation: jsonwebtoken::Validation::new(Self::ALG_FN()) })
    }
}
```
**Win:** the compiler enforces key/alg pairing; the dead RSA path is impossible to construct.

### 4.B `SupervisedTask` + `supervise()` helper (RS-CR-01)
```rust
pub struct SupervisedTask { handle: tokio::task::JoinHandle<()>, name: &'static str }
impl SupervisedTask {
    pub fn abort(&self) { self.handle.abort(); }
    pub async fn join(self) -> Result<(), tokio::task::JoinError> { self.handle.await }
}
pub fn supervise<F>(name: &'static str, fut: F) -> SupervisedTask
where F: std::future::Future<Output = ()> + Send + 'static {
    let handle = tokio::spawn(async move {
        if let Err(panic) = std::panic::AssertUnwindSafe(fut).catch_unwind().await {
            tracing::error!(task = name, ?panic, "supervised task panicked");
        }
    });
    SupervisedTask { handle, name }
}
```
Replace every bare `tokio::spawn(async move { ... })` with `supervise("axum-serve", async move { ... })`.

### 4.C `TypedRegistry<K, V>` (RS-HI-04)
```rust
pub struct TypedRegistry<K, V> { inner: dashmap::DashMap<K, std::sync::Arc<V>> }
impl<K: Eq + std::hash::Hash, V: Send + Sync + 'static> TypedRegistry<K, V> {
    pub fn get(&self, k: &K) -> Option<std::sync::Arc<V>> { self.inner.get(k).map(|e| e.clone()) }
    pub fn get_or_init<F>(&self, k: K, f: F) -> std::sync::Arc<V>
    where F: FnOnce() -> V {
        self.inner.entry(k).or_insert_with(|| std::sync::Arc::new(f())).clone()
    }
}
```
Migrate the 5 ad-hoc registries to this single impl.

### 4.D LLM crate split (RS-HI-05)
```text
rskit-llm-traits/        // pub trait Provider; pub struct ChatRequest/Response; NO deps
rskit-llm/               // depends on rskit-llm-traits only
rskit-llm-providers/     // depends on rskit-llm-traits only; impl Provider for OpenAi etc.
```
Cycle resolved; `cargo doc --all-features` becomes deterministic.

### 4.E Async `LazyComponent` (RS-ME-04)
```rust
pub struct LazyComponent<T> { cell: tokio::sync::OnceCell<std::sync::Arc<T>> }
impl<T: Send + Sync + 'static> LazyComponent<T> {
    pub async fn get_or_try_init<F, Fut, E>(&self, init: F) -> Result<std::sync::Arc<T>, E>
    where F: FnOnce() -> Fut, Fut: std::future::Future<Output = Result<T, E>> {
        self.cell.get_or_try_init(|| async { init().await.map(std::sync::Arc::new) }).await.cloned()
    }
}
```

### 4.F `LossyBroadcast` (RS-HI-08)
```rust
pub struct LossyBroadcast<T> { rx: tokio::sync::broadcast::Receiver<T>, dropped: metrics::Counter }
impl<T: Clone> LossyBroadcast<T> {
    pub async fn recv(&mut self) -> Option<T> {
        loop { match self.rx.recv().await {
            Ok(v) => return Some(v),
            Err(tokio::sync::broadcast::error::RecvError::Lagged(n)) => {
                self.dropped.increment(n);
                tracing::warn!(dropped = n, "broadcast lag");
            },
            Err(tokio::sync::broadcast::error::RecvError::Closed) => return None,
        } }
    }
}
```

### 4.G `[workspace.lints]` block (RS-HI-27, RS-HI-30)
See §6 for the full block.

### 4.H `NonZeroU32` for retry config (RS-ME-01)
```rust
pub struct RetryConfig { max_attempts: std::num::NonZeroU32, backoff: BackoffStrategy }
```
`with_max_attempts(0)` becomes a compile error.

### 4.J Typestate `HttpServer<Stopped|Bound|Running>` (RS-CR-04)
```rust
pub struct HttpServer<S> { _state: std::marker::PhantomData<S>, /* fields */ }
pub struct Stopped; pub struct Bound { listener: tokio::net::TcpListener } pub struct Running { handle: SupervisedTask }

impl HttpServer<Stopped> {
    pub async fn bind(self, addr: std::net::SocketAddr) -> Result<HttpServer<Bound>, BindError> { /* ... */ }
}
impl HttpServer<Bound> {
    pub fn start(self, app: axum::Router) -> HttpServer<Running> {
        let handle = supervise("http-serve", async move {
            if let Err(e) = axum::serve(self.listener, app).await {
                tracing::error!(?e, "http serve exited");
            }
        });
        HttpServer { _state: std::marker::PhantomData, handle }
    }
}
impl HttpServer<Running> {
    pub async fn shutdown(self) -> Result<(), tokio::task::JoinError> { self.handle.join().await }
}
```

### 4.K `AuthMode` enum + `WWW-Authenticate` (RS-HI-10)
```rust
pub enum AuthMode { Required, Optional, Bypass }
// On 401: response.headers_mut().insert(WWW_AUTHENTICATE, HeaderValue::from_static("Bearer realm=\"rskit\""));
```

### 4.L `IntoResponse for AppError` (RS-HI-14, dim2 redesign)
```rust
impl axum::response::IntoResponse for AppError {
    fn into_response(self) -> axum::response::Response {
        let problem = ProblemDetail::from(&self);
        let mut res = (self.status_code(), axum::Json(problem)).into_response();
        if matches!(self.code(), ErrorCode::Unauthorized) {
            res.headers_mut().insert(axum::http::header::WWW_AUTHENTICATE,
                axum::http::HeaderValue::from_static("Bearer realm=\"rskit\""));
        }
        res
    }
}
```

### 4.M `HealthRegistry` + `/livez` & `/readyz` (RS-HI-16)
```rust
#[async_trait::async_trait]
pub trait Probe: Send + Sync { async fn check(&self) -> ProbeResult; fn name(&self) -> &'static str; }
pub struct HealthRegistry { probes: Vec<std::sync::Arc<dyn Probe>> }
impl HealthRegistry {
    pub fn router(self: std::sync::Arc<Self>) -> axum::Router {
        axum::Router::new()
            .route("/livez", axum::routing::get(|| async { axum::http::StatusCode::OK }))
            .route("/readyz", axum::routing::get({
                let reg = self.clone();
                move || async move { /* iterate probes; aggregate */ }
            }))
    }
}
```

### 4.N Classifier `wrap()` (RS-HI-14)
```rust
pub type ErrorClassifier = std::sync::Arc<dyn Fn(&(dyn std::error::Error + 'static)) -> Option<ErrorCode> + Send + Sync>;
pub fn register_classifier(c: ErrorClassifier) { /* push to global Vec via OnceLock<RwLock<...>> */ }
impl AppError {
    pub fn wrap<E: std::error::Error + Send + Sync + 'static>(err: E) -> Self {
        let code = CLASSIFIERS.iter().find_map(|c| c(&err)).unwrap_or(ErrorCode::Internal);
        AppError { code, source: std::sync::Arc::new(err) }
    }
}
```

### 4.O Idempotent `Telemetry::init()` / `Telemetry::shutdown()` (RS-HI-17, RS-LO-05)
```rust
pub struct Telemetry { tracer_provider: opentelemetry_sdk::trace::TracerProvider }
static INIT: std::sync::OnceLock<Telemetry> = std::sync::OnceLock::new();
impl Telemetry {
    pub fn init(cfg: TelemetryConfig) -> &'static Telemetry {
        INIT.get_or_init(|| {
            opentelemetry::global::set_text_map_propagator(
                opentelemetry::propagation::TextMapCompositePropagator::new(vec![
                    Box::new(opentelemetry_sdk::propagation::TraceContextPropagator::new()),
                    Box::new(opentelemetry_sdk::propagation::BaggagePropagator::new()),
                ]));
            Telemetry { tracer_provider: build_provider(cfg) }
        })
    }
    pub async fn shutdown() {
        if let Some(t) = INIT.get() { t.tracer_provider.shutdown().await; }
    }
}
```

### 4.P `Arc<dyn Error>` for `AppError: Clone` (RS-HI-15)
```rust
#[derive(Clone)]
pub struct AppError { code: ErrorCode, source: std::sync::Arc<dyn std::error::Error + Send + Sync> }
```

### 4.Q `Clock` trait (RS-HI-23)
```rust
pub trait Clock: Send + Sync { fn now(&self) -> std::time::SystemTime; fn instant(&self) -> tokio::time::Instant; }
pub struct SystemClock; impl Clock for SystemClock { /* ... */ }
#[cfg(any(test, feature = "test-util"))]
pub struct MockClock { /* ... */ }
```

### 4.R `cargo-fuzz` skeleton (RS-HI-21)
```text
fuzz/
  Cargo.toml          # [package] name = "rskit-fuzz"
  fuzz_targets/
    auth_jwt_decode.rs
    http_request_parse.rs
```

### 4.S Criterion bench skeleton (RS-HI-24)
```rust
// crates/rskit-http/benches/router.rs
use criterion::{Criterion, criterion_group, criterion_main};
fn bench_route(c: &mut Criterion) {
    c.bench_function("route_dispatch_static", |b| b.iter(|| /* ... */));
}
criterion_group!(benches, bench_route);
criterion_main!(benches);
```

### 4.T Remove `ratelimit.rs:359` UB cast (RS-HI-26)
Replace generic `RateLimiter<O>` cross-cast with a non-generic enum boundary:
```rust
pub enum AnyLimiter { TokenBucket(TokenBucketLimiter), Leaky(LeakyBucketLimiter), Fixed(FixedWindowLimiter) }
impl AnyLimiter { pub async fn try_acquire(&self) -> Result<(), Limited> { /* match self */ } }
```


---

## §5. CI Blueprint (drop-in YAML)

### 5.A `.github/workflows/ci.yml`
```yaml
name: ci
on:
  push: { branches: [main] }
  pull_request:
permissions: { contents: read }
concurrency:
  group: ci-${{ github.workflow }}-${{ github.ref }}
  cancel-in-progress: true
env:
  CARGO_TERM_COLOR: always
  RUSTFLAGS: "-D warnings"
  RUSTDOCFLAGS: "-D warnings"
jobs:
  fmt:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@11bd71901bbe5b1630ceea73d27597364c9af683 # v4.2.2
      - uses: dtolnay/rust-toolchain@b3b07ba8b418998c39fb20f53e8b695cdcc8de1b # 1.91.0
        with: { components: rustfmt }
      - run: cargo fmt --all -- --check
  clippy:
    runs-on: ${{ matrix.os }}
    strategy:
      fail-fast: false
      matrix: { os: [ubuntu-latest, ubuntu-24.04-arm, macos-latest, windows-latest] }
    steps:
      - uses: actions/checkout@11bd71901bbe5b1630ceea73d27597364c9af683 # v4.2.2
      - uses: dtolnay/rust-toolchain@b3b07ba8b418998c39fb20f53e8b695cdcc8de1b # 1.91.0
        with: { components: clippy }
      - uses: Swatinem/rust-cache@98c8021b550208e191a6a3145459bfc9fb29c4c0 # v2.7.1
      - uses: taiki-e/install-action@9ba3ac3fd006a70c6e186a683577abc1ccf0ff3a # v2.62.43
        with: { tool: cargo-hack }
      - run: cargo hack clippy --feature-powerset --depth 2 --workspace --all-targets --locked -- -D warnings
  test:
    runs-on: ${{ matrix.os }}
    strategy:
      fail-fast: false
      matrix:
        os: [ubuntu-latest, ubuntu-24.04-arm, macos-latest, windows-latest]
        rust: ["1.85", "stable"]
    steps:
      - uses: actions/checkout@11bd71901bbe5b1630ceea73d27597364c9af683 # v4.2.2
      - uses: dtolnay/rust-toolchain@b3b07ba8b418998c39fb20f53e8b695cdcc8de1b # ${{ matrix.rust }}
        with: { toolchain: ${{ matrix.rust }} }
      - uses: Swatinem/rust-cache@98c8021b550208e191a6a3145459bfc9fb29c4c0 # v2.7.1
      - uses: taiki-e/install-action@9ba3ac3fd006a70c6e186a683577abc1ccf0ff3a # v2.62.43
        with: { tool: cargo-nextest }
      - env: { INSTA_UPDATE: "no" }
        run: cargo nextest run --workspace --all-features --locked --no-fail-fast --profile ci
      - if: failure()
        uses: actions/upload-artifact@26f96dfa697d77e81fd5907df203aa23a56210a8 # v4.3.0
        with: { name: nextest-${{ matrix.os }}-${{ matrix.rust }}, path: target/nextest/ci/ }
  msrv:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@11bd71901bbe5b1630ceea73d27597364c9af683 # v4.2.2
      - uses: taiki-e/install-action@9ba3ac3fd006a70c6e186a683577abc1ccf0ff3a # v2.62.43
        with: { tool: cargo-msrv }
      - run: cargo msrv verify --output-format json
  docs:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@11bd71901bbe5b1630ceea73d27597364c9af683 # v4.2.2
      - uses: dtolnay/rust-toolchain@b3b07ba8b418998c39fb20f53e8b695cdcc8de1b # nightly
        with: { toolchain: nightly }
      - run: cargo doc --workspace --all-features --no-deps --document-private-items
  coverage:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@11bd71901bbe5b1630ceea73d27597364c9af683 # v4.2.2
      - uses: dtolnay/rust-toolchain@b3b07ba8b418998c39fb20f53e8b695cdcc8de1b # 1.91.0
        with: { components: llvm-tools-preview }
      - uses: taiki-e/install-action@9ba3ac3fd006a70c6e186a683577abc1ccf0ff3a # v2.62.43
        with: { tool: cargo-llvm-cov }
      - run: cargo llvm-cov --workspace --all-features --lcov --output-path lcov.info --fail-under-lines 70
      - uses: codecov/codecov-action@015f24e6818733317a2da2edd6290ab26238649a # v5.0.7
        with: { files: lcov.info, fail_ci_if_error: true }
  deny:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@11bd71901bbe5b1630ceea73d27597364c9af683 # v4.2.2
      - uses: EmbarkStudios/cargo-deny-action@34899fc7ba81ca6268d5947a7a16b4649013fea1 # v2.0.4
        with: { command: check }
  semver:
    runs-on: ubuntu-latest
    if: github.event_name == 'pull_request'
    steps:
      - uses: actions/checkout@11bd71901bbe5b1630ceea73d27597364c9af683 # v4.2.2
      - uses: obi1kenobi/cargo-semver-checks-action@5b298c9520f7096a4683c0bd981a7ac5a7e249ae # v2.6
  ci-status:
    needs: [fmt, clippy, test, msrv, docs, coverage, deny]
    runs-on: ubuntu-latest
    steps: [{ run: "echo ok" }]
```

### 5.B `.github/workflows/audit-cron.yml`
```yaml
name: audit
on:
  schedule: [{ cron: "0 6 * * *" }]
  workflow_dispatch:
permissions: { contents: read, issues: write }
jobs:
  audit:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@11bd71901bbe5b1630ceea73d27597364c9af683 # v4.2.2
      - uses: rustsec/audit-check@69366f33c96575abad1ee0dba8212993eecbe998 # v2.0.0
        with: { token: ${{ secrets.GITHUB_TOKEN }} }
```

### 5.C `.github/workflows/codeql.yml`
```yaml
name: codeql
on:
  push: { branches: [main] }
  pull_request:
  schedule: [{ cron: "0 4 * * 1" }]
permissions: { security-events: write, contents: read, actions: read }
jobs:
  analyze:
    runs-on: ubuntu-latest
    strategy: { matrix: { language: [actions] } }   # add 'rust' once GA in your tier
    steps:
      - uses: actions/checkout@11bd71901bbe5b1630ceea73d27597364c9af683
      - uses: github/codeql-action/init@b56ba49b26e50535fa1e7f7db0f4f7b4bf65d80d # v3.27.5
        with: { languages: ${{ matrix.language }} }
      - uses: github/codeql-action/analyze@b56ba49b26e50535fa1e7f7db0f4f7b4bf65d80d # v3.27.5
```

### 5.D `.github/workflows/fuzz.yml`
```yaml
name: fuzz-nightly
on:
  schedule: [{ cron: "0 3 * * *" }]
  workflow_dispatch:
permissions: { contents: read }
jobs:
  fuzz:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@11bd71901bbe5b1630ceea73d27597364c9af683
      - uses: dtolnay/rust-toolchain@b3b07ba8b418998c39fb20f53e8b695cdcc8de1b # nightly
        with: { toolchain: nightly }
      - run: cargo install cargo-fuzz --locked
      - run: |
          for t in $(cargo fuzz list); do
            cargo fuzz run "$t" -- -max_total_time=600 -timeout=30
          done
```

### 5.E `.github/workflows/release.yml`
```yaml
name: release
on:
  push: { branches: [main] }
permissions: { contents: write, pull-requests: write, id-token: write, attestations: write }
jobs:
  release-plz:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@11bd71901bbe5b1630ceea73d27597364c9af683
        with: { fetch-depth: 0 }
      - uses: dtolnay/rust-toolchain@b3b07ba8b418998c39fb20f53e8b695cdcc8de1b # stable
        with: { toolchain: stable }
      - uses: MarcoIeni/release-plz-action@c54d4e442c1fbb88716de70c81dc34ec6deeae2c # v0.5
        env:
          GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}
          CARGO_REGISTRY_TOKEN: ${{ secrets.CARGO_REGISTRY_TOKEN }}
  attest:
    needs: release-plz
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@11bd71901bbe5b1630ceea73d27597364c9af683
      - uses: anchore/sbom-action@61119d458adab75f756bc0b9e4bde25725f86a7a # v0.17.7
        with: { format: cyclonedx-json, output-file: sbom.cdx.json }
      - uses: actions/attest-build-provenance@7668a9f2db40f9b4b817edcba637999e1b89dbec # v2.0.1
        with: { subject-path: 'sbom.cdx.json' }
      - uses: sigstore/cosign-installer@dc72c7d5c4d10cd6bcb8cf6e3fd625a9e5e537da # v3.7.0
      - run: cosign sign-blob --yes sbom.cdx.json --output-signature sbom.cdx.json.sig
```

### 5.F `.github/dependabot.yml`
```yaml
version: 2
updates:
  - package-ecosystem: cargo
    directory: "/"
    schedule: { interval: weekly }
    open-pull-requests-limit: 10
    cooldown: { default-days: 7, semver-major-days: 30 }
    groups:
      patch-and-minor: { update-types: [patch, minor] }
      tokio:    { patterns: ["tokio*"] }
      tracing:  { patterns: ["tracing*", "opentelemetry*"] }
      axum-tonic: { patterns: ["axum*", "tonic*", "hyper*", "tower*"] }
  - package-ecosystem: github-actions
    directory: "/"
    schedule: { interval: weekly }
    cooldown: { default-days: 7 }
    groups: { actions-all: { patterns: ["*"] } }
```

### 5.G `release-plz.toml`
```toml
[workspace]
changelog_update = true
git_release_enable = true
git_tag_enable = true
publish = true
semver_check = true
[changelog]
header = "# Changelog\n\nAll notable changes documented here.\n"
```

---

## §6. Lint Blueprint

### 6.A Workspace `Cargo.toml`
```toml
[workspace.lints.rust]
unsafe_op_in_unsafe_fn = "deny"
unused_must_use        = "deny"
missing_docs           = "warn"
missing_debug_implementations = "warn"
unreachable_pub        = "warn"
unused_lifetimes       = "warn"
unused_macro_rules     = "warn"
single_use_lifetimes   = "warn"
trivial_casts          = "warn"
trivial_numeric_casts  = "warn"

[workspace.lints.clippy]
# correctness / safety
unwrap_used        = "deny"
expect_used        = "deny"
panic              = "deny"
todo               = "deny"
unimplemented      = "deny"
dbg_macro          = "deny"
mem_forget         = "deny"
lossy_float_literal = "deny"
let_underscore_must_use = "deny"
wildcard_dependencies   = "deny"
exit               = "deny"
print_stdout       = "deny"
print_stderr       = "deny"
# style/perf
needless_borrow    = "warn"
redundant_clone    = "warn"
inefficient_to_string = "warn"
# pedantic opt-ins
must_use_candidate = "warn"
missing_errors_doc = "warn"
missing_panics_doc = "warn"

[workspace.lints.rustdoc]
broken_intra_doc_links     = "deny"
private_intra_doc_links    = "warn"
bare_urls                  = "warn"
```

### 6.B Per-crate `Cargo.toml`
```toml
[lints]
workspace = true
```

### 6.C `clippy.toml`
```toml
cognitive-complexity-threshold = 25
too-many-arguments-threshold   = 6
type-complexity-threshold      = 200
disallowed-methods = [
  { path = "std::env::set_var",   reason = "use a config builder" },
  { path = "std::env::remove_var", reason = "use a config builder" },
  { path = "tokio::spawn",         reason = "use rskit_core::supervise() so panics are logged" },
]
disallowed-types = [
  { path = "std::sync::Mutex", reason = "prefer parking_lot::Mutex or tokio::sync::Mutex" },
]
```

### 6.D Tooling additions in CI
- `cargo machete --with-metadata`
- `cargo udeps --workspace --all-features` (nightly job)
- `cargo public-api --diff-git-checkouts $(git merge-base origin/main HEAD) HEAD`

---

## §7. Roadmap

### Milestone v0.x — Cleanup (1 sprint, ~2 weeks)
Goal: green CI, no UB, releasable artefact, governance bootstrapped.
1. [RS-HI-29] `cargo fmt --all` and add fmt-check as required.
2. [RS-HI-28/RS-HI-36/RS-HI-45] Decide MSRV (recommend 1.85) and align `rust-toolchain.toml`; bump `[workspace.package].rust-version` only for documented reason.
3. [RS-HI-30 + clippy errors] Fix the 4 clippy errors in `rskit-discovery/consul.rs:118,187,217` (`io_other_error`) and `rskit-media-ffmpeg/probe/detect.rs:170` (`explicit_counter_loop`); add `[workspace.lints]` block per §6.A.
4. [RS-HI-26 / RS-ME-14] Delete the unsafe cast at `rskit-ratelimit/src/limiter.rs:359` (redesign §4.T).
5. [RS-HI-03] Delete `unsafe impl Send/Sync` in `rskit-messaging/src/middleware.rs:27-30`; add proper `where T: Send + Sync` bounds.
6. [RS-CR-01 + RS-CR-04 + RS-HI-02] Introduce `SupervisedTask` + `supervise()` (§4.B), retrofit the 5 detached spawns; typestate `HttpServer` (§4.J).
7. [RS-CR-05/06/07] SHA-pin every action; replace `dtolnay@master`; add workflow-level `permissions: contents: read` and `concurrency:` block.
8. [RS-CR-08/11 + RS-HI-43/44/45/46/47] Add `SECURITY.md`, `.github/CODEOWNERS`, `MAINTAINERS`, `.editorconfig`, `.gitattributes`, `.pre-commit-config.yaml`.
9. [RS-HI-37 / RS-CR-10] Add `dependabot.yml`; fix every internal `[workspace.dependencies]` entry to include `version =`. Run `cargo publish -p <leaf-crate> --dry-run` to verify topo order works.
10. Resolve the 7 RUSTSEC vulns (`rsa`, `rustls-webpki`, `rand`, `lru`, `paste`, `rustls-pemfile`, `number_prefix`) by upgrading their pulling crates.

**Exit criteria:** all of `cargo build --locked --all-features`, `cargo clippy --workspace --all-features -- -D warnings`, `cargo test --workspace --all-features`, `cargo fmt --check`, `cargo deny check`, `cargo audit -D warnings` pass green on three OSes. Cut tag `v0.1.0`.

### Milestone v0.y — Redesign (3 sprints, ~6 weeks)
Goal: API quality reaches the bar of `tokio` / `tower` / `tonic`.
- [RS-CR-02 + RS-HI-09/10] Typestate JWT (§4.A); decide OIDC: implement or drop the README claim. Add `WWW-Authenticate` header.
- [RS-CR-03] Wire TLS in `rskit-server` (§4.J pattern); add `ServerTls` typestate.
- [RS-HI-14/15 + RS-ME-20/21/22] Classifier `wrap()` (§4.N), `Arc<dyn Error>` for `Clone` (§4.P), per-crate sentinel error enums.
- [RS-HI-04 + RS-ME-05] Consolidate 5 registries into `TypedRegistry` (§4.C).
- [RS-HI-05] LLM crate split (§4.D).
- [RS-HI-16 + RS-ME-12/26] `HealthRegistry` + `/livez` + `/readyz` (§4.M).
- [RS-HI-17/18 + RS-ME-23/24/25 + RS-LO-05/06] `Telemetry::init()`/`shutdown()` façade (§4.O); `MaskingLayer<S>` instead of post-format masking; strip query in spans.
- [RS-HI-12 + RS-HI-13] `secrecy::SecretString` for all secrets; `AesGcm::new(&[u8;32])` only.
- [RS-HI-19/20/21/22/23] Coverage gate (`cargo llvm-cov --fail-under-lines 70`); multi_thread codemod; `cargo-fuzz` skeleton; `Clock` trait (§4.Q).
- [RS-HI-24/25] Criterion benches in `rskit-http`, `rskit-grpc-server`, `rskit-cache`, `rskit-mq`, `rskit-auth`; bench-regression gate.
- [RS-HI-39/41/42] `docs/adr/` + `#![warn(missing_docs)]` workspace-wide + crate-level rustdoc minimum.

**Exit criteria:** zero "blunt-funnel" error mappings; `/livez`+`/readyz` return real status; coverage ≥70% workspace; benches stable across CI runs; cargo-public-api baselines committed.

### Milestone v1.0 — Stabilisation (2 sprints, ~4 weeks)
Goal: OSS supply-chain ready; bus factor ≥ 2.
- [RS-CR-09 + RS-HI-49/50] Full release pipeline with cosign + SBOM + SLSA L3; `cargo-public-api` and `cargo-semver-checks` as required checks.
- Per-crate coverage floor: 80% (auth/encryption: 90%).
- `SECURITY.md` PGP key + Private Vulnerability Reporting opt-in verified end-to-end.
- Bus factor recruitment: ≥2 maintainers in `MAINTAINERS`; document on-boarding.
- Branch protection: required CI status (`ci-status`), required reviews (1+), require signed commits.
- `cargo-vet` baselines for transitive review (RS-LO-20).
- Document SemVer + Deprecation policy (`docs/SEMVER.md`, `docs/DEPRECATION.md`).
- `actionlint` + `zizmor` in pre-commit + CI.

**Exit criteria:** `cargo install rskit-cli@1.0` works from a fresh machine on Linux/macOS/Windows × x86_64/aarch64; SECURITY.md vuln channel acknowledged within 48h; CI from PR-open to merge ≤ 12 min p95.

---

## §8. Open Questions

1. **Branch protection** — cannot verify from a clone; please share screenshot of repo Settings → Branches.
2. **MSRV vs channel decision** — pin to `1.85` (max library compat) or `1.91` (use newer features)? The current incoherent state (lib claims 1.85, toolchain pins 1.91, CI runs 1.85) is the worst of both.
3. **49-crate split justification** — at 49 crates the `cargo build` cost is non-trivial; is every crate justified by a downstream consumer? Candidates for consolidation: `rskit-uri` + `rskit-url`, `rskit-component` + `rskit-di`, possibly `rskit-bench` (rename) + `rskit-evals`.
4. **Lockstep vs per-crate versioning** — recommendation is lockstep `0.x.y` until v1.0 (matches current `[workspace.package]` shape). After v1.0, evaluate per-crate.
5. **`cargo publish` dry-run pending** — RS-CR-10 fix must be verified end-to-end before any public release; estimate ~half day to walk the topo order.
6. **Bus factor recruitment plan** — who are the candidate co-maintainers? Document explicitly in `MAINTAINERS` with email + scope.
7. **22 unsafe blocks** — beyond the two flagged (`rskit-messaging` + `rskit-ratelimit`), the remaining 20 unsafe blocks in the workspace need a one-time audit; document each with a `// SAFETY: ...` comment per the [workspace.lints] `unsafe_op_in_unsafe_fn = "deny"` rule.
8. **Top-level docs sprawl** — `MEDIA_IMPLEMENTATION.md` (2463 lines) plus other markdown at root pollute the README context. Recommend `docs/design/`.

---

## §9. Final Verdict

**Status:** **NOT READY for v1.0.** 11 Critical and ~50 High findings block public release. The internal quality is real (rustls-only, tracing-everywhere, ConstantTimeEq, Argon2id, RFC-9457 problem details, single-source workspace metadata, two-version Rust matrix) but the **edges** that an OSS consumer audits first — release pipeline, security disclosures, governance, supply-chain hygiene, JWT correctness, TLS wiring — are the parts that are weakest.

**Severity counts:** Critical 11 · High 50 · Medium 50 · Low 25 · Nit 4 = **140 findings**.

**Top-5 blocker checklist (must clear before any v1.0 announcement):**
- [ ] **RS-CR-01** Supervise every detached `tokio::spawn` (5 sites).
- [ ] **RS-CR-02** Typestate `JwtAlgo` so RS256/384/512 cannot fall through to HMAC.
- [ ] **RS-CR-03** Wire TLS in `rskit-server` (currently plaintext gRPC).
- [ ] **RS-CR-04** `HttpServer` typestate so bind/serve errors propagate (no `unwrap` in detached spawn).
- [ ] **RS-CR-10** Add `version =` to every internal `[workspace.dependencies]` entry; verify `cargo publish --dry-run` topo-walks the DAG.

**Plus, before any release:** clear the 7 cargo-audit advisories (`rsa` Marvin, `rustls-webpki` ×2 RUSTSECs, `rand`, `lru`, `paste`, `rustls-pemfile`, `number_prefix`), add `SECURITY.md` + `CODEOWNERS`, fix `cargo fmt --check`, fix the 4 `cargo clippy --all-features` errors, decide and align MSRV.

**Once those are green** the project is in genuinely good shape: the architecture is sensible, tests pass, the dependency choices are deliberate, and the redesigns proposed in §4 are mechanical, not philosophical. With a focused 2-sprint cleanup the project can credibly cut a `v0.1.0` baseline; with a further 6 weeks of redesign work and 4 weeks of stabilisation, `rskit` can stand alongside `tokio` / `axum` / `tonic` as a recommended OSS dependency.

