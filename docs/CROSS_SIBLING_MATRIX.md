# Cross-Sibling Parity & Divergence Matrix — gokit · rskit · pykit

> Inputs: `review-gokit.md` (111 findings · 1 Critical / 30 High / 48 Med / 31 Low / 1 Nit),
> `review-rskit.md` (140 findings · 11 Crit / 50 High / 50 Med / 25 Low / 4 Nit),
> `review-pykit.md` (144 findings · 11 Crit / 60 High / 53 Med / 19 Low / 1 Nit).
>
> Finding-ID conventions:
> - **gokit** uses `F-NNN` in the aggregate review (e.g. `F-013`). Per-dimension reports use `CQ-`/`AR-`/`CC-`/`SC-`/`ER-`/`OB-`/`TS-`/`PF-`/`LT-`/`CI-`/`TC-`/`DC-`/`RL-`/`RH-` prefixes; this document standardises on the **`F-NNN`** aggregate IDs and parenthesises the dimension code where it improves clarity.
> - **rskit** uses `RS-CR-NN` / `RS-HI-NN` / `RS-ME-NN` / `RS-LO-NN` / `RS-NI-NN`.
> - **pykit** uses `PY-CR-NN` / `PY-HI-NN` / `PY-ME-NN` / `PY-LO-NN` / `PY-NI-NN`.
> - All IDs in this file have been verified to exist in the underlying review documents (no invented IDs).
>
> **Important asymmetry, called out once and assumed throughout:** pykit's `pykit-server` is **gRPC-only**; it ships **no HTTP framework adapter** (`pykit-server-middleware` is hand-rolled raw-ASGI; FastAPI/Starlette integration is not provided). Wherever a row reads "HTTP server" or "HTTP middleware" for pykit, the cell is either filled with the gRPC analogue or marked **N/A (gRPC-only)** + the surrogate gRPC location.

---

<a id="sec-1"></a>
## 1. Concept Parity Matrix

Cell format: `path:Lstart-Lend (status)`. `status` is one of:
- ✅ present and broadly equivalent
- ⚠️ present but with a flagged defect (finding ID inline)
- ❌ missing
- N/A — concept does not apply to this sibling
- ❓ — present but path/lines not captured in dim files

### 1.A Errors & Problem Details

| Concept | gokit | rskit | pykit |
|---|---|---|---|
| `AppError` type | `errors/errors.go` ✅ ⚠️ flat-cause leak (F-035) | `rskit-error/src/lib.rs` ✅ ⚠️ `!Clone` (RS-HI-15), no classifier (RS-HI-14) | `packages/pykit-errors/src/pykit_errors/` ✅ ⚠️ `__str__` leaks cause (PY-HI-22) |
| `ErrorCode` taxonomy | `errors/codes.go` ✅ | `rskit-error/src/lib.rs` ✅ `#[non_exhaustive]` win | `pykit-errors/.../codes.py` ✅ |
| ProblemDetails (RFC 7807/9457) | `errors/response.go:14-16` ⚠️ `init()` global (F-031) | `rskit-error/src/lib.rs` ✅ bidirectional w/ `tonic::Status` | `pykit-errors/.../problem.py` ⚠️ auth shape diverges (PY-ME-20) |
| `Wrap` / typed classifier | `errors/errors.go` ⚠️ collapses to 500 (F-034) | `rskit-error/src/lib.rs:188-220` ⚠️ single match → 500 (RS-HI-14) | ❌ no classifier (PY-HI-23) |
| `init()` / module-globals on URI base | `errors/response.go:14-16` ⚠️ (F-031) | `rskit-uri/src/lib.rs:38-56` ⚠️ `OnceLock<String>` (RS-ME-13) | `pykit-errors/.../_TYPE_BASE_URI` ⚠️ (PY-HI-21) |
| Cause-chain preservation | ⚠️ `%v`-flatten not `errors.Join` (F-074) | ✅ `Box<dyn Error>` (`Arc` proposed RS-HI-15) | ⚠️ 117 `raise X` w/o `from e` (PY-HI-03) |

### 1.B Component / Lifecycle / Bootstrap

| Concept | gokit | rskit | pykit |
|---|---|---|---|
| Component lifecycle | `component/registry.go` ⚠️ StartAll write-lock (F-078) | `rskit-component/src/lazy.rs:88-122` ⚠️ Mutex serialises init (RS-ME-04) | `packages/pykit-component/src/pykit_component/lifecycle.py` ⚠️ no rollback on partial failure (PY-HI-07) |
| Bootstrap entry point | `bootstrap/app.go`, `bootstrap/summary.go:211` ⚠️ ANSI to writer (F-042); hard shutdown deadline (F-075) | ❓ no dedicated `rskit-bootstrap`; lifecycle composed via `rskit-component` + service crates | `packages/pykit-bootstrap/` ❓ (presence asserted; no dim flagged defects) |
| Lazy/Once init | n/a | `rskit-component/src/lazy.rs:88-122` ⚠️ should be `OnceCell::get_or_try_init` (RS-ME-04) | n/a (asyncio-native) |

### 1.C Registry pattern (count + form)

| Concept | gokit | rskit | pykit |
|---|---|---|---|
| Number of ad-hoc registries | **6** — `auth/registry.go`, `discovery/component.go`, `storage/factory.go`, `tool/registry.go`, `workload/factory.go`, `llm/registry.go` (F-015) | **5** — `rskit-di/src/registry.rs`, `rskit-component/src/registry.rs`, `rskit-config/src/registry.rs`, `rskit-messaging/src/registry.rs`, `rskit-llm/src/registry.rs` (RS-HI-04) | **3** module-level mutable (PY-HI-06); plus racy `_REGISTRY` w/o lock (PY-ME-12) |
| Duplicate-key policy consistency | ❌ five different policies (panic / overwrite / error) (F-015) | ❌ divergent (RS-HI-04) | ❌ inconsistent (PY-HI-06) |
| Generic typed `Registry[K,V]` | ❌ proposed `internal/registry` (F-015 §4.2) | ❌ proposed `TypedRegistry<K,V>` in `rskit-core` (RS-HI-04 §4.C) | ❌ proposed (PY redesign RP-1) |

### 1.D DI / Container

| Concept | gokit | rskit | pykit |
|---|---|---|---|
| DI container | `di/container.go:29-46,304-336` ⚠️ stringly-typed `interface{}` god-object (F-013) | `rskit-di/src/container.rs:140-178` ⚠️ per-key `OnceLock` duplicates init (RS-ME-05) | `packages/pykit-container/src/pykit_container/container.py:120-160` ⚠️ unsafe `T` cast (PY-HI-01); process-global cycle-detect (PY-HI-08) |
| `Must*` / panic helpers | 9+ sites (F-016) — `MustGet/MustResolve` | `unwrap()`/`expect()` 714 sites (RS-HI-01) | 8 `assert` in lib (PY-HI-04) + `# noqa: F821` masking (PY-HI-02) |

### 1.E Provider abstraction · Pipeline / DAG

| Concept | gokit | rskit | pykit |
|---|---|---|---|
| Provider abstraction | `provider/streaming.go:71` ⚠️ leak (F-008) | LLM provider trait — ⚠️ cyclic `rskit-llm ↔ rskit-llm-providers` (RS-HI-05) | `pykit-provider/` ✅; covered by mypy (one of 5 strict-checked) |
| Pipeline / DAG engine | `pipeline/`, `chain/` ✅ ⚠️ misspellings (F-105) | ❓ no dedicated DAG crate flagged | `pykit-pipeline/` ⚠️ DAG `gather` w/o `return_exceptions=True` (PY-HI-12); `TaskGroup` migration (PY-ME-29) |

### 1.F Validation · Encryption · Security primitives

| Concept | gokit | rskit | pykit |
|---|---|---|---|
| Validation | `validation/struct_validator.go:39,44` ⚠️ errorlint type-assertion (F-083) | ❓ no dedicated `rskit-validation` flagged (likely embedded in `rskit-error`/`serde`) | `pykit-validation/` ✅ (untyped sigs PY-ME-04 sibling pattern) |
| Encryption (AES-GCM) | `security/` ✅ | `rskit-encryption/src/aes.rs:44-78` ⚠️ key-stretch via SHA-256 (RS-HI-13) | `pykit-encryption` ⚠️ SHA-256 KDF, no salt/AAD (PY-ME-15); AES-GCM 96-bit nonce ✅ |
| TLS config knob | `security/tls.go` ⚠️ `MinVersion` excluded by `hasSettings` (F-040) | `rskit-httpclient/src/client.rs:42-78` ⚠️ no TLS knobs surfaced (RS-ME-16) | `pykit-tls`/`TLSConfig.is_enabled()` ⚠️ excludes `min_version` (PY-HI-18) |
| Constant-time compare | ⚠️ no `subtle.ConstantTimeCompare` audit | ✅ `subtle::ConstantTimeEq` in `rskit-auth/src/apikey.rs` | ✅ `hmac.compare_digest` (where present) |
| Secret zeroisation | n/a | ⚠️ `zeroize` declared, never derived (RS-HI-12) | n/a (Python GC) |
| Password hashing | n/a | ✅ Argon2id only | ⚠️ `HashAlgorithm.ARGON2` is **scrypt** (PY-HI-15) |

### 1.G HTTP server · gRPC server · Server Middleware

| Concept | gokit | rskit | pykit |
|---|---|---|---|
| HTTP server | `server/` (Gin) ✅ | `rskit-http/src/server.rs:88-102` ⚠️ panics in detached spawn (RS-CR-04) | **N/A (gRPC-only)** — `pykit-server-middleware` is raw-ASGI; no FastAPI/Starlette adapter (PY-CR-04 callout) |
| gRPC server | `grpc/`, `grpc/resolver/discovery` ⚠️ 0% coverage (F-017) | `rskit-grpc-server/src/server.rs:71-95` ⚠️ unsupervised spawn (RS-CR-01) + plaintext (RS-CR-03) | `packages/pykit-server/src/pykit_server/base.py:25-119` ⚠️ `add_insecure_port` only, no TLS (PY-HI-19) |
| Server middleware | `server/middleware/*.go` ⚠️ `gin.H` errors not ProblemDetail (F-032), tracing alloc (F-047) | `rskit-http/src/middleware/trace.rs:24-58` ⚠️ raw query in spans (RS-HI-18); CORS not default (RS-ME-11) | `pykit-server-middleware/` ⚠️ tracing/prometheus/ratelimit/tenant only — **no AuthMiddleware, no /healthz, no CORS, no CSRF** (PY-CR-04) |

### 1.H Auth (JWT, OIDC, API key) · Authorization

| Concept | gokit | rskit | pykit |
|---|---|---|---|
| JWT verify | `auth/jwt/` ⚠️ HMAC secret length unchecked (F-041) | `rskit-auth/src/jwt.rs:140-198` ⚠️ **RSA path silently runs HMAC** (RS-CR-02) | `pykit-auth-jwt/` ⚠️ no leeway, no HMAC length, exposes `decode_unverified` (PY-HI-14) |
| OIDC ID-token verifier | `auth/oidc/verifier.go:95-112` ⚠️ alg-confusion (F-002), no skew (F-036), no nonce check (F-037) | ❌ docs claim OIDC; **no code path** (RS-HI-09) | ❌ **no verifier exists** — only `refresh_token` client (PY-CR-03) |
| JWKS cache | `auth/oidc/jwks.go` ⚠️ no single-flight (F-038) | ❌ N/A (no OIDC) | ❌ no JWKS cache (PY-CR-03 fix) |
| API key | `auth/apikey/` ⚠️ 17.9% coverage (F-017) | `rskit-auth/src/apikey.rs:54-92` ⚠️ no `WWW-Authenticate`, bypass-before-reject (RS-HI-10); ✅ `subtle::ConstantTimeEq` | `pykit-api-key`/middleware ⚠️ swallows errors → generic 401 (PY-ME-17) |
| Auth middleware (HTTP) | `server/middleware/auth.go:90,96,128` ⚠️ no ProblemDetail (F-032), missing-vs-invalid conflated (F-033), query-token leak (F-039) | `rskit-http` ⚠️ no `WWW-Authenticate`, no `AuthMode` (RS-HI-10) | ❌ **does not exist** (PY-CR-04) |
| Authz (RBAC/policy) | ❓ no explicit module flagged | ❓ no flagged authz module | ❓ no flagged authz module |

### 1.I Resilience (retry / circuit-breaker / rate-limit)

| Concept | gokit | rskit | pykit |
|---|---|---|---|
| Retry | `resilience/` (degradation.go:82) ✅ | `rskit-retry/src/lib.rs:62-78` ⚠️ `with_max_attempts(0)` panics (RS-ME-01) | ❓ |
| Circuit breaker | `resilience/` ✅ | ❓ | ❓ |
| Rate limiter | ❓ | `rskit-ratelimit/src/limiter.rs:359` ⚠️ **UB cast** between generic instantiations (RS-HI-26); Drop spawns w/o await (RS-ME-10) | `pykit-rate-limit` ⚠️ `_cleanup_task` ref dropped (PY-HI-10), `threading.Lock` mixed w/ asyncio (PY-HI-11), reaches into private state (PY-LO-01) |

### 1.J Observability (logging / tracing / metrics)

| Concept | gokit | rskit | pykit |
|---|---|---|---|
| Logger / structured logging | `logger/logger.go:39` ⚠️ `zerolog.SetGlobalLevel` global (F-011) | `rskit-logging/src/init.rs:38-72` ⚠️ `RUST_LOG` overrides config + post-format masking (RS-HI-11) | `pykit-logging/` ⚠️ `print()` fallback (PY-ME-02 / OB-05) |
| Tracer init | `observability/tracer.go` ⚠️ globals not idempotent, ignore caller ctx (F-043, F-045) | `rskit-tracing/src/init.rs:62-94` ⚠️ no global propagator + no façade (RS-HI-17) | `pykit-telemetry/setup_tracing` ⚠️ globals (PY-HI-24); no exporter wired (PY-HI-25); no propagator (PY-HI-26) |
| Meter init | `observability/meter.go:55` ⚠️ globals (F-043) | included in RS-HI-17 façade gap | `setup_metrics` ⚠️ (PY-HI-24) |
| Span attribute helper | `observability/span.go` ⚠️ silently drops unsupported types (F-044) | `rskit-tracing/src/init.rs:42-64` ⚠️ Resource missing service.version etc. (RS-ME-24) | `ErrorHandlingInterceptor` ⚠️ no `record_exception` (PY-ME-26) |
| HTTP RED metrics | ⚠️ `tracing.go:28` per-req `Sprintf` (F-047) | ❌ no built-in `MetricsLayer` (RS-ME-25) | `PrometheusMiddleware` ⚠️ unbounded `path` cardinality (PY-ME-19); same for `OperationMetrics` (PY-ME-24) |
| Telemetry façade (idempotent init/shutdown) | ❌ proposed (F-043) | ❌ proposed `Telemetry::init/shutdown` (RS-HI-17 §4.O) | ❌ proposed (PY-HI-24) |

### 1.K Health (`/livez`, `/readyz`)

| Concept | gokit | rskit | pykit |
|---|---|---|---|
| Health taxonomy types | `observability/health.go` ✅ ⚠️ no timestamp/latency (F-110) | `rskit-server/src/health.rs:28-44` ⚠️ hard-coded `Ok(true)` (RS-ME-26 / RS-HI-37); no `checked_at`/`latency` (RS-LO-06) | `pykit-health/` ✅ types only ⚠️ no `checked_at`/`latency_ms` (PY-LO-05) |
| HTTP `/healthz`/`/readyz` handler | ❌ **not shipped** (F-030) | ❌ **not shipped** (RS-HI-16) | ❌ **not shipped** (PY-CR-05) |

### 1.L Discovery · Worker · SSE / Streaming

| Concept | gokit | rskit | pykit |
|---|---|---|---|
| Service discovery | `discovery/component.go:34-87` ⚠️ panic-helpers (F-016) | `rs-services/rskit-discovery/src/consul.rs:118,187,217,241-265` ⚠️ clippy errors + unsupervised heartbeat spawn (RS-CR-01, baseline tooling-red) | `packages/pykit-discovery/` ⚠️ **10 failing tests** + ctor drift + layer violation (PY-HI-33 / PY-CR-02) |
| Worker / pool | `worker/keyedpool.go` ⚠️ lock through Submit (F-076); `worker/sse_bridge.go:9` ⚠️ layering inversion (F-012) | n/a explicit `worker` crate; spawn supervision tracked (RS-CR-01) | `pykit-worker` ⚠️ pool not actually limited at submit (PY-ME-13) |
| SSE / streaming | `sse/hub.go` ⚠️ unguarded `Broadcast` send leak (F-010); `provider/streaming.go:71` ⚠️ (F-008); `agent/agent.go:191-260` ⚠️ (F-009) | `rskit-mq/src/broker.rs:188-210` ⚠️ broadcast `Lagged` silent (RS-HI-08) + spawn (RS-CR-01) | `LocalSource.fetch` ⚠️ sync I/O in async (PY-HI-13) |

### 1.M LLM / Agent / MCP / Media

| Concept | gokit | rskit | pykit |
|---|---|---|---|
| LLM stack | `llm/registry.go` ⚠️ (F-015) | `rskit-llm/Cargo.toml` ⚠️ cyclic w/ providers (RS-HI-05) | `pykit-llm/` ✅ |
| Agent stack | `agent/agent.go:191-260` ⚠️ leak (F-009); `agent/prompt.go:74,77,168` ⚠️ panic helpers (F-016) | ❓ | `pykit-agent/` ❓ (referenced in pip-audit failure PY-HI-17) |
| MCP | `mcp/` ⚠️ no fuzz (F-018) | ❓ | ❓ |
| Media | `media/media.go:88` (toolchain-CVE reachability F-001); no `doc.go` (F-062) | `rskit-media-ffmpeg/src/probe/detect.rs:170` ⚠️ clippy `explicit_counter_loop` baseline-red | `pykit-media` ⚠️ wants atheris fuzz (PY-HI-44) |

### 1.N Storage · Database · Redis · Messaging

| Concept | gokit | rskit | pykit |
|---|---|---|---|
| Storage / object factory | `storage/factory.go:30-39` ⚠️ panics (F-015, F-016) | ❓ | ❓ |
| Database | ❓ | ❓ | `pykit-database` ⚠️ untyped sigs (PY-ME-04); core not strict-typed (PY-CR-01) |
| Redis | ❓ | ❓ | ❓ |
| Messaging (Kafka/etc.) | `messaging/.../consumer.go` ⚠️ Stop ignores ctx (F-077); orphan `kafka/v0.2.0` tag (F-098) | `rskit-messaging/src/middleware.rs:27-30` ⚠️ unsound `unsafe impl Send + Sync` (RS-HI-03) | `pykit_messaging.kafka` ⚠️ wants atheris (PY-HI-44) |
| Cache | n/a | `rskit-cache/src/registry.rs:84-118` ⚠️ `tokio::Mutex` across non-await (RS-HI-06); `manager.rs:144-160` unsupervised spawn (RS-CR-01) | n/a |

### 1.O Testing · Bench · Datasets / Workload

| Concept | gokit | rskit | pykit |
|---|---|---|---|
| Test count / status baseline | 252 test files, logger flake (F-011) | 2 470 tests pass; 4 clippy errors + 23 fmt hunks; `cargo test` not nextest (RS-HI-32) | 10 unit failures in `pykit-discovery` (PY-HI-33); `pytest-xdist` not installed → serial (PY-HI-28) |
| Coverage | per-pkg holes: `auth/oidc 13.1%`, `auth/apikey 17.9%`, `server/endpoint 0%` (F-017) | no coverage gate, no llvm-cov (RS-HI-19) | global 90.81% but `fail_under = 60`; 3 pkgs below 80% (PY-HI-29) |
| Fuzz | 5 targets; missing JWT/JWKS/OIDC/etc. (F-018) | `cargo-fuzz` 0; `proptest` 0; `loom` 0 (RS-HI-21) | `hypothesis`/`atheris` absent (PY-HI-32, PY-HI-44) |
| Integration tag | 0 `//go:build integration` (F-019) | 1 `#[ignore]` stub (RS-HI-22) | 0 `@pytest.mark.integration` (PY-HI-30) |
| Determ. clock | 101 `time.Now()`, 0 Clock (TS-05 referenced) | no `Clock` trait, real wall-clock (RS-HI-23) | 90 wall-clock sites; no `freezegun` (PY-HI-31) |
| Bench (perf) | 5 `Benchmark*` (F-020); no `benchstat` gate (F-021); `bench/` is model-eval misnamed (F-081) | 2/49 crates have benches (RS-HI-24); no regression gate (RS-HI-25); `rskit-bench` is ML-evals misnamed (RS-ME-28) | no `pytest-benchmark`/`pyperf` (PY-HI-34); naming collision (PY-LO-10) |
| Datasets / Workload | `workload/factory.go:57` ⚠️ panic (F-016) | `rskit-dataset` (RS-NI-02 inverted feature) | `pykit-dataset 68.4%` coverage (PY-HI-29); `pykit-workload` ❓ |
| Multi-thread runtime tests | n/a | 0/820 `#[tokio::test]` use `multi_thread` (RS-HI-20) | n/a (asyncio single-loop) |

### 1.P Telemetry init/shutdown · MSRV

| Concept | gokit | rskit | pykit |
|---|---|---|---|
| Idempotent telemetry façade | ❌ (F-043) | ❌ (RS-HI-17) | ❌ (PY-HI-24) |
| MSRV / language pin | `go 1.26.0` w/ 8 stdlib CVEs; no `toolchain` directive (F-001/F-061) | `rust-toolchain.toml = 1.91` vs `rust-version = 1.85` mismatch (RS-HI-28/RS-HI-36/RS-ME-06) | `python-version` policy not pinned across pkgs (PY-HI-47); no `requires-python` discipline |

---

<a id="sec-2"></a>
## 2. Recurring Anti-Patterns (cross-cutting)

For each smell: instances per sibling with finding IDs, then the unified pattern recommendation.

### 2.1 Multiple inconsistent registry implementations
- gokit: **F-015** (6 implementations, 5 different policies)
- rskit: **RS-HI-04** (5 implementations)
- pykit: **PY-HI-06** (3 module-level mutable) + **PY-ME-12** (`_REGISTRY` racy)
- **Unified pattern.** One `Registry[K, V]` primitive in the lowest tier crate (`internal/registry` / `rskit-core` / `pykit-registry`). Standard contract: `register(name, v) -> Result<()>` (error on duplicate, never panic), `get(name) -> Result<V>` (error on missing), `names() -> Vec<String>`. Every domain registry (`auth`, `tool`, `storage`, `llm`, `discovery`, …) is a thin newtype wrapper.

### 2.2 Module-level mutable state (`init()` / `static` / `_REGISTRY` / `OnceLock`)
- gokit: **F-031** (`errors/response.go init()` mutates URI base)
- rskit: **RS-ME-13** (`rskit-uri OnceLock<String>`), **RS-LO-07** (`rskit-config RwLock<Option<String>>`)
- pykit: **PY-HI-21** (`_TYPE_BASE_URI`), **PY-ME-12** (`_REGISTRY`), **PY-HI-08** (`Container._resolving` set process-global)
- **Unified pattern.** No mutable module/process globals. Configuration is constructed once by `bootstrap` and threaded through DI/Container. Where a process-singleton is unavoidable (e.g. tracer provider), wrap in an idempotent `Telemetry::init() -> &'static Telemetry` returning the same handle on second call.

### 2.3 Detached background tasks without supervision
- gokit: **F-008** (`provider/streaming.go:71`), **F-009** (`agent/agent.go:191-260`), **F-010** (`sse/hub.go Broadcast`), **F-076** (`worker/keyedpool.go SubmitOrAttach`), **F-077** (`messaging/.../consumer.go Stop`)
- rskit: **RS-CR-01** (5 unsupervised `tokio::spawn` in http/grpc/discovery/mq/cache), **RS-CR-04** (`HttpServer::axum::serve(..).await.unwrap()` inside spawn), **RS-ME-10** (rate-limiter `Drop` spawns)
- pykit: **PY-HI-09** (`asyncio.create_task(self.stop())` fire-and-forget), **PY-HI-10** (`RateLimiter._cleanup_task` ref dropped), **PY-HI-12** (`gather` w/o `return_exceptions`), **PY-ME-10** (lost-task catalog), **PY-ME-13** (`WorkerPool` not pool-limited at submit)
- **Unified pattern.** Three rules: (a) every spawn returns a tracked `Supervised{Task,Goroutine,Coroutine}` handle owned by a parent scope; (b) every channel send is guarded by `select … <-ctx.Done()` / `tokio::select!{ _ = cancel.cancelled() => … }` / `asyncio.TaskGroup`; (c) panics in supervised tasks are captured + logged + propagated to `/readyz`.

### 2.4 Layered-architecture boundary violations
- gokit: **F-012** (`worker → sse` import inversion); **F-014** (no `depguard`)
- rskit: **RS-HI-05** (`rskit-llm ↔ rskit-llm-providers` cyclic); permissive `deny.toml` (**RS-ME-07**)
- pykit: **PY-CR-02** (import-linter contract broken — `pykit_grpc → pykit_discovery`; only 39/55 packages in contract)
- **Unified pattern.** Three formal tiers: **foundation** (errors/logging/config/validation, no internal deps) → **core** (component/registry/di/telemetry) → **transport/integration** (http/grpc/messaging/llm/storage). Enforced mechanically: `depguard` (Go), `deny.toml` + `cargo-deny` graph (Rust), `import-linter` enumerating all packages (Python). Contract ships with **independence** rules for the foundation tier.

### 2.5 Missing typed error classifier (`Wrap`)
- gokit: **F-034** (`Wrap` collapses `context.DeadlineExceeded`/`sql.ErrNoRows` to 500)
- rskit: **RS-HI-14** (`AppError::wrap` single-match → 500)
- pykit: **PY-HI-23** (no `Wrap`; every caller does `isinstance(e, AppError) else AppError.internal(e)` by hand)
- **Unified pattern.** Pluggable classifier registry: `register_classifier(Fn(&Error) -> Option<ErrorCode>)`. Each crate registers its own (DB, HTTP client, FS, JSON…); `Wrap` walks the registry and only falls back to `Internal` when no classifier matches.

### 2.6 Missing OIDC discovery + JWKS single-flight
- gokit: ⚠️ verifier exists but `F-002` (alg-confusion), `F-036` (no clock skew), `F-037` (no nonce check), `F-038` (no JWKS single-flight; DoS amplifier)
- rskit: ❌ **no OIDC code path** (RS-HI-09)
- pykit: ❌ **no verifier exists** at all (PY-CR-03)
- **Unified pattern.** Ship a `verifier` crate per language with: alg pinned to JWKS `jwk.alg`; allow-list per issuer; clock skew leeway; `iat`/`nbf`/`exp` enforcement; nonce parameter (required iff configured); JWKS cache with single-flight on `kid` miss + bounded refetch rate (DoS guard); spans around discovery + JWKS HTTP.

### 2.7 Missing `/healthz` & `/readyz` HTTP handlers
- gokit: **F-030** (`Health` taxonomy exists, no handler)
- rskit: **RS-HI-16** / **RS-ME-12** / **RS-ME-26** (hard-coded `Ok(true)`); `RS-LO-06` (no `checked_at`/`latency`)
- pykit: **PY-CR-05** (no handler); `PY-LO-05` (no `checked_at`/`latency_ms`)
- **Unified pattern.** `HealthRegistry` with pluggable `Probe` trait/interface; `/livez` returns 200 unless terminating; `/readyz` aggregates probes; per-component result includes `name, status, latency_ms, checked_at, error?`. Mounted by `bootstrap` automatically.

### 2.8 Tracer/Meter/Logging not idempotent + no shutdown
- gokit: **F-043** (`InitTracer`/`InitMeter` mutate globals, second call races); **F-045** (ignore caller ctx)
- rskit: **RS-HI-17** (no global `TextMapPropagator`, no façade); **RS-LO-05** (sync shutdown in `Drop`)
- pykit: **PY-HI-24** (`setup_*` globals; second call wins, no shutdown returned); **PY-HI-25** (no exporter wired); **PY-HI-26** (no propagator)
- **Unified pattern.** Single `Telemetry` façade with `init(cfg) -> &Telemetry` (idempotent — `OnceLock`/`OnceCell`/sync.Once equivalent) and `async shutdown()` returning all flush errors. Sets `TraceContextPropagator` + `BaggagePropagator` composite. Resource fields (`service.{name,version,instance.id}`, `deployment.environment`) populated from env.

### 2.9 No SHA pinning of GitHub Actions
- gokit: **F-003** (`securego/gosec@master`), **F-004** (0/10 actions SHA-pinned)
- rskit: **RS-CR-05** (0/24 SHA-pinned), **RS-CR-06** (`dtolnay/rust-toolchain@master`), **RS-CR-07** (no workflow-level `permissions:`)
- pykit: **PY-HI-39** (12 mutable `@v4`/`@v5`); **PY-CR-07** umbrella
- **Unified pattern.** All `uses:` pinned to 40-char SHA + version comment. Workflow-level `permissions: contents: read` baseline; per-job upgrades only as needed (`id-token: write` for OIDC publishing, `security-events: write` for SARIF). Add `actionlint` + `zizmor` pre-commit + CI jobs.

### 2.10 No SBOM / cosign / CodeQL
- gokit: **F-007** (no release/CodeQL/SBOM/cosign/SLSA workflows)
- rskit: **RS-HI-35** (no CodeQL); **RS-ME-19** (no SBOM/cosign/SLSA/CodeQL); **RS-HI-49** (no signed tags)
- pykit: **PY-CR-07** (no security scanning); **PY-HI-56** (no SBOM/Sigstore); **PY-CR-10** (no Trusted Publishing OIDC)
- **Unified pattern.** Three workflows per repo: `codeql.yml` (language-appropriate); `release.yml` with CycloneDX SBOM (`anchore/sbom-action`) + Sigstore keyless (`sigstore/cosign-installer` + `actions/attest-build-provenance` for SLSA); `vuln.yml` scheduled scan with SARIF upload to code-scanning.

### 2.11 No release pipeline
- gokit: **F-007**, **F-025** (no `.goreleaser.yml`/cosign/SLSA), **F-026** (23 tags, 0 GitHub Releases), **F-027** (CHANGELOG ends `[0.1.5]`), **F-067** (lockstep tag-modules.sh defeats multi-module SemVer)
- rskit: **RS-CR-09** (no release pipeline at all); **RS-CR-10** (`cargo publish` will reject path-only sibling deps); **RS-HI-49** (0 tags / 0 Releases / no SemVer doc)
- pykit: **PY-CR-06** (no release workflow; nothing ever shipped); **PY-CR-11** (all 55 pkgs `0.1.0`); **PY-HI-54/55** (no SemVer policy / signed tags / release-notes)
- **Unified pattern.** Adopt `goreleaser` (Go) / `release-plz` (Rust) / `release-please` (Python). Per-module/crate/package independent SemVer; signed tags; cosign keyless; SBOM attestation; PyPI Trusted Publishing for pykit.

### 2.12 Missing SECURITY / CODEOWNERS / MAINTAINERS / GOVERNANCE
- gokit: **F-029** (bus factor 1 — `MAINTAINERS.md` + `CODEOWNERS` only `@kbukum`); **F-101** (CODEOWNERS missing 11 packages); SECURITY.md exists (one of the wins)
- rskit: **RS-CR-08** (no `SECURITY.md`); **RS-CR-11** (no `CODEOWNERS`); **RS-HI-43**/**RS-HI-44** (no `MAINTAINERS`/`GOVERNANCE`)
- pykit: **PY-HI-50** (no `SECURITY.md`); **PY-HI-51** (README falsely cites missing `CODE_OF_CONDUCT.md`); **PY-HI-52** (no `MAINTAINERS`/`GOVERNANCE`/`CODEOWNERS`); **PY-HI-60** (no `CONTRIBUTING.md`); **PY-HI-58** (no ISSUE/PR templates); **PY-HI-59** (no CHANGELOG)
- **Unified pattern.** Identical community-health file set across all three repos: `SECURITY.md` (PGP + Private Vulnerability Reporting), `MAINTAINERS.md` (≥2 humans), `CODEOWNERS` (every top-level dir), `GOVERNANCE.md`, `CONTRIBUTING.md` w/ DCO, `CODE_OF_CONDUCT.md`, `CHANGELOG.md`, `ISSUE_TEMPLATE/{bug,feature}.yml`, `PULL_REQUEST_TEMPLATE.md`, `FUNDING.yml` (optional), `.github/dependabot.yml`.

### 2.13 Lockfile / toolchain pinning gaps
- gokit: **F-061** (`go.work` `go 1.26.0` no separate `toolchain`); **F-051** (CI golangci-lint v2.0.0 vs dev v2.9.0); **F-088** (CONTRIBUTING says Go 1.25.0)
- rskit: **RS-HI-38** (`cargo build` w/o `--locked`); **RS-ME-36** (no `--locked`)
- pykit: **PY-CR-09** (`uv.lock` gitignored); **PY-HI-43** (no `uv lock --check`/`uv sync --locked`)
- **Unified pattern.** Lockfile committed; CI uses `--locked` / `--frozen` / `uv sync --locked`; tool versions pinned with comments; Dependabot (or Renovate) drives bumps.

### 2.14 MSRV / `rust-version` / `requires-python` not enforced
- gokit: **F-001** (Critical — `go 1.26.0` w/ 8 stdlib CVEs, no toolchain pin); **F-061**
- rskit: **RS-HI-28** (`1.91` toolchain vs `1.85` MSRV; clippy passes on 1.85, fails on 1.91); **RS-LO-16** (no `cargo-msrv` job)
- pykit: **PY-HI-47** (some pkgs allow 3.11, some 3.13)
- **Unified pattern.** Single MSRV declared workspace-wide; CI matrix includes the MSRV plus stable; verification job (`cargo msrv verify` / Go vuln scan / `requires-python` linter) on every PR.

---

<a id="sec-3"></a>
## 3. Where Each Sibling Wins

### 3.1 gokit wins (over rskit + pykit)
1. **JWT alg pinned at `Validation::new(algo)`** — at least no alg-confusion at the JWT lib boundary (rskit RS-CR-02 silently HMACs RSA; pykit has no verifier).
2. **OIDC verifier *exists*** (`auth/oidc/verifier.go`) — flagged for hardening but the file ships; rskit RS-HI-09 has only docs claim, pykit PY-CR-03 has nothing.
3. **44/45 packages have `doc.go`** (F-062) — only `media/` missing; rskit has 19 skeletal `lib.rs` (RS-HI-41), pykit has zero per-package READMEs (PY-HI-53).
4. **34-module split with consistent `go 1.26.0` tidy across all** — multi-module SemVer scaffolding present (even if F-067 lockstep is broken, rskit RS-CR-10 can't even publish).
5. **Per-module CI matrix + per-module Codecov flags** — granular signal beyond what rskit (RS-HI-19 has no coverage) or pykit (PY-HI-41 doesn't upload) ship.
6. **5 `Fuzz*` targets exist** — small but >0 (rskit RS-HI-21 = 0; pykit PY-HI-32 = 0).
7. **`SECURITY.md`, `MAINTAINERS.md`, `GOVERNANCE.md` all present** — rskit lacks all three (RS-CR-08, RS-HI-43/44), pykit lacks all three (PY-HI-50/52).
8. **`CHANGELOG.md` follows Keep-a-Changelog shape** — rskit RS-LO-23 has none; pykit PY-HI-59 has none.
9. **`tag-modules.sh` exists** — broken (F-067) but a non-trivial multi-module release scaffold beyond rskit/pykit baselines.
10. **Race+shuffle by default in test job** — surfaces real flakes (the very `F-011` zerolog flake was caught here); rskit RS-HI-20 doesn't even run multi-thread runtimes.

### 3.2 rskit wins (over gokit + pykit)
1. **`subtle::ConstantTimeEq` on API-key compare** in `rskit-auth/src/apikey.rs` — gokit auth/apikey doesn't audit constant-time, pykit has it only opportunistically.
2. **rustls-only across the workspace** — no `native-tls`, no OpenSSL anywhere; gokit F-040 still has a `MinVersion` bug, pykit PY-HI-18 mirrors it.
3. **`tracing` everywhere with zero `println!`/`eprintln!` in library code** — pykit PY-ME-02 has `print()` in logging fallback; gokit F-042 writes ANSI to writer.
4. **JWT `Validation::new(algo)` correctly pins alg per token** at the lib boundary (separate from RS-CR-02 which is the PEM/secret confusion).
5. **Argon2id is the only password hash** — pykit PY-HI-15 ships `ARGON2`-named-but-actually-scrypt.
6. **`#[non_exhaustive]` on public `ErrorCode` enum** (RS-ME-02 catalogues remaining gaps but `ErrorCode` is the win).
7. **Single-source `[workspace.package]` + `[workspace.dependencies]`** — gokit has 34 individual `go.mod`, pykit has 55 individual `pyproject.toml`.
8. **Two-version Rust matrix `[1.85, stable]`** — gokit F-053 runs single Go version, pykit PY-HI-40 runs single Python.
9. **`46/49` crates carry `[package.metadata.docs.rs]`** — neither sibling has comparable docs-publication metadata.
10. **`deny.toml` with `unknown-registry = "deny"` + `unknown-git = "deny"`** — supply-chain root-of-trust gates beyond gokit (no equivalent) and pykit (PY-CR-08).

### 3.3 pykit wins (over gokit + rskit)
1. **Ruff format & lint pass clean at HEAD** — neither gokit (F-024 lint exit=0 hides 15 issues) nor rskit (`cargo fmt` 23 hunks, 4 clippy errors) is green.
2. **mypy + import-linter both configured** (PY-CR-01/02 are about *scope* and a *broken* contract, not *absence*); gokit has no equivalent type-coverage gate, rskit lacks `[workspace.lints]` (RS-HI-27).
3. **88/88 `# type: ignore` are scoped with `[code]`** — disciplined silencing.
4. **53/55 `__init__.py` declare `__all__`** — explicit public surface, beyond what gokit/rskit's package declarations provide.
5. **Global coverage 90.81%** — even with the `fail_under=60` gate (PY-HI-29), the actual measured coverage is the highest of the three.
6. **`hmac.compare_digest` + AES-GCM 96-bit random nonce** used correctly where present.
7. **No `pickle`, no `yaml.load(...)`, no `shell=True`, no SQL string interpolation in core lib** — explicit deny baseline rskit/gokit don't articulate.
8. **`async`-first I/O throughout** — only one notable lapse (PY-HI-13 sync I/O in `async def`); gokit's concurrency story is goroutine sprawl.
9. **`pydantic` validation models in many packages** — schema enforcement at the type boundary that gokit's struct tags + rskit's `serde` don't match for runtime ergonomics.
10. **`uv` workspace + hatch backend** — modern Python toolchain end-to-end (the gap is `uv.lock` not committed, PY-CR-09).

---

<a id="sec-4"></a>
## 4. Severity Comparison

| Sibling | Critical | High | Medium | Low | Nit | Total |
|---|---|---|---|---|---|---|
| **gokit**  | 1  | 30 | 48 | 31 | 1 | 111 |
| **rskit**  | 11 | 50 | 50 | 25 | 4 | 140 |
| **pykit**  | 11 | 60 | 53 | 19 | 1 | 144 |
| **Total** (deduped against parity) | 23 | 140 | 151 | 75 | 6 | **395** |

### Aggregate verdict

- **gokit** is the **gold standard** in shape: 1 Critical (toolchain-only) and the smallest finding count. Most of its weight is in Highs around supply-chain hygiene + concurrency leak hotspots; design intent is coherent, but the lifecycle automation is incomplete.
- **rskit** has the largest **Critical surface** (11) — a mix of supply-chain (`@master` action, no SHA pin, no `permissions:`, no SECURITY/CODEOWNERS, no release pipeline, path-only deps blocking publish) **and** runtime-correctness (RS-CR-01..04: 5 unsupervised spawns, JWT-RSA-as-HMAC, gRPC plaintext, HttpServer panics-in-spawn). Baseline tooling is **red** (clippy fails, fmt fails, audit fails).
- **pykit** has **the highest High count (60)** and ties rskit on Criticals (11). The defining gap is **scope**: `pykit-server` is gRPC-only, no HTTP middleware adapter, no OIDC verifier, no `/healthz`. Process is mature in places (ruff/mypy/import-linter scaffolded) but **broken at HEAD** (mypy crashes, import-linter contract violated, 10 unit tests red, `uv.lock` gitignored, all 55 packages stuck at `0.1.0`).

**None of the three is v1.0-ready.** The blocking class is identical across siblings: supply-chain hygiene + lifecycle automation + at least one runtime-correctness CVE-class issue. Stabilising one without the others would be wasted work — Section 5 sequences the lock-step plan.

---

<a id="sec-5"></a>
## 5. Coordinated Roadmap

Cross-cutting milestones to land in lockstep across all three repos. Each (a–f) is a single epic with parallel per-sibling owner-scope.

### (a) Telemetry façade + Health registry — *Milestone: `obs-v1`*
- **gokit owner-scope.** Replace `observability/tracer.go`+`meter.go` global mutators with `observability/telemetry` façade (idempotent `Init`, `Shutdown(ctx)`); ship `observability/healthhttp.Registry` (sketch in F-030 §4.6) wired by `bootstrap`. Resolves F-030, F-043, F-044, F-045, F-110.
- **rskit owner-scope.** New `rskit-telemetry` crate w/ `Telemetry::init/shutdown` (RS-HI-17 §4.O); set composite propagator. New `rskit-health` crate w/ `HealthRegistry` + `Probe` trait + `/livez`+`/readyz` axum router (RS-HI-16 §4.M). Populate Resource (RS-ME-24). Resolves RS-HI-16/17, RS-ME-24/25/26, RS-LO-05/06, RS-HI-37.
- **pykit owner-scope.** Refactor `pykit-telemetry/setup_*` to `Telemetry` class returning shutdown handle (PY-HI-24); wire OTLP exporter (PY-HI-25); set `TextMapPropagator` (PY-HI-26). New ASGI `pykit_health.HealthRegistry` (PY-CR-05) + gRPC `grpc.health.v1.Health` adapter; add `checked_at`/`latency_ms` (PY-LO-05); fix unbounded label cardinality (PY-ME-19, PY-ME-24); add OIDC + ErrorHandlingInterceptor spans (PY-ME-25, PY-ME-26).

### (b) Typed Registry + lifecycle Component — *Milestone: `core-v1`*
- **gokit.** `internal/registry/registry.go` (F-015 §4.2); migrate 6 ad-hoc registries; fix `worker → sse` inversion + add `depguard` tier rules (F-012, F-014); replace DI any-typed god-object with typed generics (F-013 §4.1); rewrite 9+ `Must*` to two-return-value (F-016); remove `StartAll` write-lock + hardcoded shutdown deadlines (F-075, F-078).
- **rskit.** `rskit-core::TypedRegistry<K,V>` (RS-HI-04 §4.C); migrate 5 registries; split `rskit-llm-traits` to break cycle (RS-HI-05 §4.D); `LazyComponent` via `OnceCell::get_or_try_init` (RS-ME-04 §4.E); ban `unwrap`/`expect` in lib via `[workspace.lints]` (RS-HI-01, RS-HI-30).
- **pykit.** `pykit_registry.Registry[K, V]` (PY-HI-06); fix `Container.resolve` mandatory `type_hint` (PY-HI-01); `ContextVar`-scoped `_resolving` (PY-HI-08); `Component.start_all` rollback via `TaskGroup` (PY-HI-07); fix import-linter contract + extend to 55 packages (PY-CR-02); fix the 10 red `pykit-discovery` tests (PY-HI-33).

### (c) AppError + classifier + ProblemDetails — *Milestone: `errors-v1`*
- **gokit.** `errors.Renderer` killing `init()` (F-031 §4.5); `errors.Wrap` plug-in classifier (F-034); `AppError.Error()` no longer flattens cause (F-035); switch to `errors.Join` (F-074); ProblemDetail in auth middleware (F-032); `Mode` enum + `WWW-Authenticate` (F-033, F-039 §4.3).
- **rskit.** `Arc<dyn Error>` for `AppError: Clone` (RS-HI-15 §4.P); pluggable classifier (RS-HI-14 §4.N); per-crate sentinel error types (RS-ME-20); `serde_json::Error.classify()` mapping (RS-ME-22); `IntoResponse for AppError` w/ `WWW-Authenticate` (§4.L).
- **pykit.** Strip `_TYPE_BASE_URI` global + inject via context (PY-HI-21); `AppError.__str__` no longer leaks cause (PY-HI-22); plug-in `Wrap` classifier (PY-HI-23); fold `RefreshError` into taxonomy (PY-ME-21); align auth response to ProblemDetail (PY-ME-20); enable `TRY200`/`B904` to enforce `from e` (PY-HI-03).

### (d) OIDC verifier + JWKS cache — *Milestone: `auth-v1`*
- **gokit.** Harden existing verifier: bind header `alg` to JWK `alg` + reject `none` + per-issuer allow-list (F-002); clock-skew leeway + `nbf`/`iat` (F-036); nonce check (F-037); JWKS single-flight on `kid` miss (F-038); strip query token after consumption (F-039); enforce JWT HMAC secret length (F-041); fuzz verifier + JWKS parse (F-018).
- **rskit.** New `rskit-auth-oidc` crate via `openidconnect` crate (RS-HI-09); typestate JWT key/alg (RS-CR-02 §4.A) so RSA-as-HMAC is uncompilable; `WWW-Authenticate` + `AuthMode` (RS-HI-10 §4.K); replace SHA-256 KDF with `[u8;32]`-only API (RS-HI-13); wrap secrets in `secrecy::SecretString` w/ `ZeroizeOnDrop` (RS-HI-12); fix UB cast in rate-limiter (RS-HI-26 §4.T).
- **pykit.** New `pykit_auth_oidc.verifier` (PY-CR-03) — alg-bound to JWK `alg`, allow-list, leeway, nonce; new `JWKSCache` w/ single-flight; new `pykit_server_middleware.auth.AuthMiddleware` (PY-CR-04) w/ `Mode` enum + RFC 6750 challenge; gRPC TLS via `BaseServer.add_secure_port` (PY-HI-19); fix JWT no-leeway / HMAC length / `decode_unverified` (PY-HI-14); replace mis-named `ARGON2` with real Argon2id (PY-HI-15); strip token-endpoint body from refresh exception (PY-HI-20); fix `TLSConfig.is_enabled()` `min_version` (PY-HI-18).

### (e) CI hardening (SHA-pin, SBOM, CodeQL, release pipeline) — *Milestone: `ci-v1`*
- **gokit.** SHA-pin all 10 actions + remove `@master` (F-003, F-004); pin `govulncheck` w/ SARIF upload (F-006); drop standalone gosec, rely on golangci-lint gosec (F-005, F-050); add `release.yml` (GoReleaser library mode + cosign keyless + syft SBOM), `codeql.yml`, `actionlint.yml`, scheduled `vuln.yml` (F-007); fix dependabot (F-058, F-059); cut `0.2.0` Release (F-026, F-027); SemVer + Deprecation policies (F-028); per-module independent SemVer in `tag-modules.sh` (F-067, F-068); bump Go to 1.26.2 + `toolchain` directive across all 34 manifests (**F-001 Critical**).
- **rskit.** SHA-pin all 24 actions (RS-CR-05) + replace `dtolnay/rust-toolchain@master` (RS-CR-06); workflow-level `permissions: contents: read` (RS-CR-07); add `release.yml` via `release-plz` (RS-CR-09); fix path-only sibling deps to `{ path = ..., version = "=0.x.y" }` (RS-CR-10); add `concurrency:` block (RS-HI-33); switch to `cargo nextest` (RS-HI-32); add `--locked` (RS-HI-38); add `cargo llvm-cov`+Codecov (RS-HI-19); add CodeQL (RS-HI-35); Dependabot (RS-HI-37); `cargo-hack --feature-powerset` (RS-ME-37); `cargo-semver-checks`+`cargo-public-api`+`cargo-msrv` (RS-ME-38/39, RS-LO-16); `actionlint`+`zizmor`+`dependency-review-action` (RS-ME-44/45); rust-cache (RS-ME-41); resolve all 7 RUSTSEC vulns (baseline); fix all 4 clippy errors + 23 fmt hunks (RS-HI-29); enable `[workspace.lints]` (RS-HI-27/30).
- **pykit.** SHA-pin 12 unpinned actions (PY-HI-39); add `release.yml` w/ PyPI Trusted Publishing OIDC (PY-CR-06, PY-CR-10); add Sigstore + CycloneDX SBOM (PY-HI-56); add `security.yml` (CodeQL + pip-audit + bandit) (PY-CR-07, PY-HI-16); add Dependabot (PY-CR-08); commit `uv.lock` + add `uv sync --locked` gate (PY-CR-09, PY-HI-43); add build matrix (OS × Python) (PY-HI-40); upload coverage to Codecov (PY-HI-41); install `pytest-xdist` (PY-HI-28); raise `fail_under` to 80 (PY-HI-29); add atheris/hypothesis fuzz (PY-HI-32, PY-HI-44); enable Ruff `S`+`TRY`+`PT`+`LOG`+`G`+`DTZ`+`PERF`+`C4`+`PIE`+`RET`+`T20`+`N`+`FURB`+`ANN` (PY-HI-36); fix mypy `--strict` to cover all 55 (PY-CR-01, PY-HI-42); fix the 5 transitive CVEs (cryptography/pip/pygments/pytest/python-multipart); cut `0.2.0` per-package release (PY-CR-11, PY-HI-54).

### (f) Documentation + community-health files — *Milestone: `health-v1`*
- **gokit.** Recruit ≥1 reviewer per major area (F-029); add `media/doc.go` (F-062); broaden `Example*_test.go` coverage (F-063); seed `docs/adr/` (F-064); README badges + quickstart (F-065); fix CONTRIBUTING Go version (F-088); merge VERSIONING-QUICK-FIX (F-091); add `ISSUE_TEMPLATE/config.yml` (F-100); tighten CODEOWNERS to cover all 11 missing dirs (F-101); add `.pre-commit-config.yaml` (F-070); add `actionlint`/`zizmor`/`gitleaks` workflows (F-071, F-072).
- **rskit.** `SECURITY.md` (RS-CR-08); `CODEOWNERS` (RS-CR-11); `MAINTAINERS`+`GOVERNANCE` (RS-HI-43, RS-HI-44); `.editorconfig`+`.gitattributes` (RS-HI-45, RS-HI-46); `.pre-commit-config.yaml` (RS-HI-47); branch protection audit (RS-HI-48); `docs/adr/` (RS-HI-39); move `MEDIA_IMPLEMENTATION.md` → `docs/design/media.md` (RS-HI-40); fix 19 skeletal `lib.rs` + `#![warn(missing_docs)]` workspace-wide (RS-HI-41, RS-HI-42); per-crate CHANGELOG via release-plz (RS-LO-23); ISSUE/PR templates + `FUNDING.yml` (RS-LO-24, RS-LO-25).
- **pykit.** `SECURITY.md` (PY-HI-50); fix or create `CODE_OF_CONDUCT.md` (PY-HI-51); `MAINTAINERS`+`GOVERNANCE`+`CODEOWNERS` (PY-HI-52); per-package READMEs + mkdocs/Sphinx site (PY-HI-53); `CONTRIBUTING.md` w/ DCO (PY-HI-60); `CHANGELOG.md` (PY-HI-59); ISSUE/PR templates + `FUNDING.yml` (PY-HI-58, PY-ME-50, PY-ME-51); `.pre-commit-config.yaml` (PY-HI-45); `.editorconfig`+`.gitattributes` (PY-HI-46); ARCHITECTURE.md + C4 diagrams (PY-ME-44); `docs/release-process.md` (PY-ME-45); examples/ + API reference (PY-ME-46, PY-ME-47); deprecation policy + `__deprecated__` decorator (PY-HI-57).

### Sequencing recommendation
Land in this order to keep API surface aligned: **(c) errors-v1 → (b) core-v1 → (a) obs-v1 → (d) auth-v1** (these are the API-shape epics; ship together so cross-sibling consumers can upgrade together) → then **(e) ci-v1 + (f) health-v1** in parallel (orthogonal to API surface).

---

<a id="sec-6"></a>
## 6. Cross-Sibling Backlinks (Critical & High)

Use these mappings as the `Sibling: …` line in each Phase E issue. "no parity" means the sibling has no comparable finding ID; usually that means either the concept doesn't exist there (e.g. pykit gRPC-only) or that sibling is the one that actually solved the problem.

### 6.A gokit Critical & High → siblings

| gokit | Title (abbrev) | rskit | pykit |
|---|---|---|---|
| F-001 (Crit) | Go 1.26.0 stdlib CVEs / no toolchain | RS-HI-28, RS-HI-36, RS-ME-06 (toolchain mismatch) | PY-HI-47 (python-version not pinned) |
| F-002 | OIDC alg-confusion | RS-HI-09 (OIDC missing entirely) | PY-CR-03 (no verifier) |
| F-003 | `securego/gosec@master` | RS-CR-06 (`dtolnay/rust-toolchain@master`) | PY-HI-39 (12 mutable refs) |
| F-004 | 0/10 actions SHA-pinned | RS-CR-05 (0/24) | PY-HI-39 (12) |
| F-005 | gosec excludes neutralise opt-in | RS-ME-07 (permissive `deny.toml`) | PY-ME-32 (TC001/2/3 blanket-ignored) |
| F-006 | `govulncheck@latest` non-reproducible | RS-LO-09 (cargo-audit/deny dup) | PY-HI-17 (pip-audit broken) |
| F-007 | No release/CodeQL/SBOM/cosign/SLSA | RS-CR-09 (no release), RS-HI-35 (no CodeQL), RS-ME-19 (no SBOM/cosign/SLSA) | PY-CR-06 (no release), PY-CR-07 (no security scan), PY-HI-56 (no SBOM/Sigstore) |
| F-008 | `provider/streaming.go` goroutine leak | RS-CR-01 (5 unsupervised spawns) | PY-HI-09 (fire-and-forget create_task), PY-HI-12 (gather w/o return_exceptions) |
| F-009 | `agent/agent.go` Stream leak | RS-CR-01 | PY-HI-09 |
| F-010 | `sse/hub.go` Broadcast leak | RS-HI-08 (broadcast Lagged silent) | PY-HI-13 (sync I/O in async) |
| F-011 | `zerolog.SetGlobalLevel` global | RS-HI-11 (`RUST_LOG` overrides config) | PY-HI-24 (setup_* mutate globals) |
| F-012 | `worker → sse` layering inversion | RS-HI-05 (cyclic llm ↔ providers) | PY-CR-02 (import-linter contract broken) |
| F-013 | DI stringly-typed `interface{}` | RS-ME-05 (per-key OnceLock) | PY-HI-01 (unsafe T cast), PY-HI-08 (process-global cycle-detect) |
| F-014 | No `depguard` | RS-ME-07 (permissive deny.toml) | PY-CR-02 (import-linter only 39/55) |
| F-015 | 6 inconsistent registries | RS-HI-04 (5 registries) | PY-HI-06 (3 registries), PY-ME-12 (`_REGISTRY` racy) |
| F-016 | 9+ `Must*` panic helpers | RS-HI-01 (714 unwrap/expect) | PY-HI-04 (8 asserts), PY-HI-02 (`# noqa: F821`) |
| F-017 | Coverage holes in security pkgs | RS-HI-19 (no coverage gate) | PY-HI-29 (3 pkgs <80%, gate=60) |
| F-018 | Fuzz coverage anemic | RS-HI-21 (cargo-fuzz=0) | PY-HI-32, PY-HI-44 (no atheris) |
| F-019 | No `//go:build integration` tag | RS-HI-22 (1 `#[ignore]` stub) | PY-HI-30 (no `@pytest.mark.integration`) |
| F-020 | Only 5 benchmarks | RS-HI-24 (2/49 crates) | PY-HI-34 (no pytest-benchmark/pyperf) |
| F-021 | No `benchstat` regression gate | RS-HI-25 (no bench gate) | PY-HI-34 |
| F-022 | `_test\.go` blanket lint exclusion | RS-LO-11 (clippy.toml empty), RS-ME-03 | PY-LO-11 (per-file-ignores narrow) |
| F-023 | Missing critical linters | RS-HI-30 (clippy lints not enabled), RS-HI-27 (no `[workspace.lints]`) | PY-HI-36 (Ruff misses 11 families) |
| F-024 | `lint exit=0` despite govet shadow bugs | RS-HI-29 (`cargo fmt` 23 hunks) | PY-CR-01 (mypy crashes) |
| F-025 | No `.goreleaser.yml`/cosign/SLSA | RS-CR-09 | PY-CR-06, PY-HI-56 |
| F-026 | 23 git tags, 0 GitHub Releases | RS-HI-49 (0 tags / 0 Releases) | PY-CR-11 (all 55 pkgs `0.1.0`) |
| F-027 | CHANGELOG ends `[0.1.5]` | RS-LO-23 (no per-crate changelog) | PY-HI-59 (no CHANGELOG) |
| F-028 | No SemVer / deprecation policy | RS-HI-49 | PY-HI-54, PY-HI-57 |
| F-029 | Bus factor 1 | RS-CR-08, RS-CR-11, RS-HI-43, RS-HI-44 | PY-HI-50, PY-HI-52 |
| F-030 | No `/healthz` handler | RS-HI-16 / RS-ME-12 / RS-ME-26 | PY-CR-05 |

### 6.B rskit Critical & High → siblings

| rskit | Title (abbrev) | gokit | pykit |
|---|---|---|---|
| RS-CR-01 | 5 unsupervised `tokio::spawn` | F-008/F-009/F-010, F-076, F-077 | PY-HI-09, PY-HI-10, PY-HI-12 |
| RS-CR-02 | JWT RSA path silently HMAC | F-002 (alg-confusion at boundary), F-041 (HMAC length) | PY-CR-03 (no verifier), PY-HI-14 (JWT no leeway / HMAC length) |
| RS-CR-03 | gRPC plaintext despite TlsConfig | F-040 (TLS MinVersion bug) | PY-HI-19 (`add_insecure_port` only), PY-HI-18 (TLS min_version) |
| RS-CR-04 | HttpServer panics in detached spawn | F-008/F-009/F-010 (same family) | N/A (gRPC-only); related PY-HI-09 |
| RS-CR-05 | 0/24 actions SHA-pinned | F-004 | PY-HI-39 |
| RS-CR-06 | `dtolnay/rust-toolchain@master` | F-003 (`gosec@master`) | no parity (no `@master` flagged) |
| RS-CR-07 | No workflow-level `permissions:` | F-056 (vuln job no permissions block) | PY-ME-35 (per-job permissions never downgraded) |
| RS-CR-08 | No SECURITY.md | gokit has it (win); F-029 (bus factor) | PY-HI-50 |
| RS-CR-09 | No release pipeline | F-007, F-025 | PY-CR-06 |
| RS-CR-10 | `cargo publish` will reject path-only deps | no parity (Go modules don't need this) | PY-CR-11 (all `0.1.0`, never published) |
| RS-CR-11 | No CODEOWNERS | F-101 (CODEOWNERS missing 11 dirs) | PY-HI-52 |
| RS-HI-01 | 714 `unwrap()`/`expect()` | F-016 | PY-HI-04 (asserts), PY-HI-02 |
| RS-HI-02 | `unwrap()` inside detached spawn | F-008..F-010 | PY-HI-09 |
| RS-HI-03 | Unsound `unsafe impl Send + Sync` | no parity (Go has no `unsafe impl`) | PY-HI-11 (threading.Lock + asyncio mix) |
| RS-HI-04 | 5 divergent registries | F-015 | PY-HI-06, PY-ME-12 |
| RS-HI-05 | `rskit-llm ↔ rskit-llm-providers` cyclic | F-012 (worker→sse inversion) | PY-CR-02 (import-linter broken) |
| RS-HI-06 | `tokio::Mutex` across non-await | F-076 (lock through Submit) | PY-HI-11 (threading.Lock + asyncio) |
| RS-HI-07 | `select!` without `biased;` | F-107 (fragile select) | PY-ME-11 (`shield`+`wait_for` race) |
| RS-HI-08 | broadcast `Lagged(n)` silent | F-010 (sse Broadcast unguarded) | PY-HI-12 (gather w/o return_exceptions) |
| RS-HI-09 | OIDC missing entirely | F-002, F-036, F-037, F-038 (OIDC exists but flawed) | PY-CR-03 (no verifier) |
| RS-HI-10 | API-key 401 missing `WWW-Authenticate` | F-033 (missing-vs-invalid conflated; no `WWW-Authenticate`) | PY-CR-04 (no AuthMiddleware), PY-ME-17 (APIKey swallows errors) |
| RS-HI-11 | `RUST_LOG` overrides config + post-format mask | F-011 (zerolog global) | PY-HI-24 (setup_* globals) |
| RS-HI-12 | `zeroize` declared, never derived | no parity (Go GC; secret length still F-041) | no parity |
| RS-HI-13 | AES key stretched via SHA-256 | F-040 (TLS only; no key-stretch flag) | PY-ME-15 (SHA-256 KDF, no salt/AAD) |
| RS-HI-14 | `AppError::wrap` collapses to 500 | F-034 | PY-HI-23 |
| RS-HI-15 | `AppError: !Clone` | F-074 (`%v`-flatten not `errors.Join`) | no parity (Python all-objects copyable) |
| RS-HI-16 | No `/livez`+`/readyz` | F-030 | PY-CR-05 |
| RS-HI-17 | No Telemetry façade / no propagator | F-043, F-045 | PY-HI-24, PY-HI-26 |
| RS-HI-18 | Trace span includes raw query | F-039 (query-token leak) | PY-ME-19, PY-ME-24 (cardinality) |
| RS-HI-19 | No coverage gate | F-017, F-054 | PY-HI-29, PY-HI-41 |
| RS-HI-20 | 0/820 `multi_thread` tokio tests | F-019 (no integration tag) | PY-HI-28 (xdist not installed) |
| RS-HI-21 | No `cargo-fuzz`/proptest/loom | F-018 | PY-HI-32, PY-HI-44 |
| RS-HI-22 | `#[ignore]` stub as integration plan | F-019 | PY-HI-30 |
| RS-HI-23 | No `Clock` abstraction | F-017-adjacent (TS-05 in dim3: 101 `time.Now()`) | PY-HI-31 (90 wall-clock sites) |
| RS-HI-24 | 2/49 crates have benches | F-020 | PY-HI-34 |
| RS-HI-25 | No bench-regression gate | F-021 | PY-HI-34 |
| RS-HI-26 | UB cast in rate-limiter | no parity (Go memory-safe) | PY-LO-01 (reaches private state, but not UB) |
| RS-HI-27 | No `[workspace.lints]` | F-023 (missing critical linters) | PY-HI-36 (Ruff missing 11 families) |
| RS-HI-28 | Toolchain drift 1.91 vs MSRV 1.85 | F-001/F-061 (Go 1.26.0 toolchain) | PY-HI-47 (Python version not pinned) |
| RS-HI-29 | `cargo fmt` 23 hunks | F-024 (real govet shadow bugs) | (pykit ruff format passes — pykit win) |
| RS-HI-30 | Critical clippy lints not enabled | F-023 | PY-HI-36 |
| RS-HI-31 | Matrix ubuntu/macos only — no Win/arm64 | F-053 (degraded matrix) | PY-HI-40 (single ubuntu × single Python) |
| RS-HI-32 | Tests run via `cargo test` not nextest | F-055 (Windows test no `-race`) | PY-HI-28 (xdist missing) |
| RS-HI-33 | No `concurrency:` block | (gokit unflagged — likely present) | PY-ME-37 |
| RS-HI-34 | No release workflow | F-007 | PY-CR-06 |
| RS-HI-35 | No CodeQL / no SAST | F-007 | PY-CR-07 |
| RS-HI-36 | `rust-toolchain.toml = 1.91` exact pin | F-001/F-061 | PY-HI-47 |
| RS-HI-37 | No Dependabot | F-058/F-059 (dependabot bug + missing groups) | PY-CR-08 |
| RS-HI-38 | `cargo build` w/o `--locked` | F-051 (golangci-lint version drift) | PY-CR-09 + PY-HI-43 |
| RS-HI-39 | No `docs/adr/` | F-064 | PY-ME-44 |
| RS-HI-40 | 2.5k-line MEDIA_IMPLEMENTATION.md in root | F-091 (VERSIONING-QUICK-FIX should merge) | PY-LO-17 (no glossary/concepts page) |
| RS-HI-41 | 19/49 crates skeletal `lib.rs` | F-062 (`media/doc.go` missing) | PY-HI-53 (no per-package READMEs) |
| RS-HI-42 | 20/49 crates lack `#![warn(missing_docs)]` | F-062 | PY-HI-53 |
| RS-HI-43 | No SECURITY/MAINTAINERS/GOVERNANCE | F-029 (bus factor), gokit has SECURITY (win) | PY-HI-50, PY-HI-52 |
| RS-HI-44 | No MAINTAINERS | F-029 | PY-HI-52 |
| RS-HI-45 | No `.editorconfig` | (gokit unflagged — likely present) | PY-HI-46 |
| RS-HI-46 | No `.gitattributes` | (gokit unflagged — likely present) | PY-HI-46 |
| RS-HI-47 | No pre-commit | F-070 | PY-HI-45 |
| RS-HI-48 | Branch protection unverified | F-073 | PY-ME-38 |
| RS-HI-49 | 0 tags / 0 Releases / no SemVer doc | F-026, F-028 | PY-HI-54, PY-HI-55 |
| RS-HI-50 | No `cargo-public-api` baselines | F-028-adjacent | PY-HI-57 (no deprecation) |

### 6.C pykit Critical & High → siblings

| pykit | Title (abbrev) | gokit | rskit |
|---|---|---|---|
| PY-CR-01 | mypy `--strict` 5 of 55 + crash | F-024 (lint exit=0 despite real bugs) | RS-HI-27 (no `[workspace.lints]`) |
| PY-CR-02 | Import-linter contract broken | F-012 (worker→sse), F-014 (no depguard) | RS-HI-05 (cyclic crates) |
| PY-CR-03 | No OIDC ID-token verifier | F-002, F-036, F-037, F-038 | RS-HI-09 |
| PY-CR-04 | No HTTP AuthMiddleware | F-032, F-033, F-039 | RS-HI-10 |
| PY-CR-05 | No `/healthz`/`/readyz` shipped | F-030 | RS-HI-16 |
| PY-CR-06 | No release workflow | F-007, F-025 | RS-CR-09 |
| PY-CR-07 | No security scanning | F-007 | RS-HI-35, RS-ME-19 |
| PY-CR-08 | No Dependabot | F-058 (dependabot bug) | RS-HI-37 |
| PY-CR-09 | `uv.lock` gitignored | F-061 (no toolchain directive) | RS-HI-38 (`--locked` missing) |
| PY-CR-10 | No PyPI Trusted Publishing OIDC | F-007 | RS-CR-09 |
| PY-CR-11 | All 55 pkgs `0.1.0` | F-026 (23 tags, 0 Releases) | RS-HI-49 (0 tags) |
| PY-HI-01 | `Container.resolve` unsafe T | F-013 | RS-ME-05 |
| PY-HI-02 | `# noqa: F821` masks missing import | F-024 (govet shadow bugs hidden) | RS-HI-29 (fmt failures) |
| PY-HI-03 | 117 `raise X` w/o `from e` | F-074 (`%v`-flatten not `errors.Join`) | RS-LO-03 (cause chain dropped on serialise) |
| PY-HI-04 | 8 `assert` in lib code | F-016 | RS-HI-01 |
| PY-HI-05 | 99 `Any` in non-test (46 files) | no parity (Go is statically typed) | no parity |
| PY-HI-06 | 3 module-level mutable registries | F-015 | RS-HI-04 |
| PY-HI-07 | `Component.start_all` no rollback | F-078 (StartAll write-lock) | RS-ME-04 (LazyComponent serialises) |
| PY-HI-08 | `Container._resolving` process-global | F-031 (errors init globals) | RS-ME-13 (`OnceLock<String>`) |
| PY-HI-09 | `create_task(self.stop())` fire-and-forget | F-008/F-009/F-010 | RS-CR-01, RS-CR-04 |
| PY-HI-10 | `RateLimiter._cleanup_task` ref dropped | F-077 (consumer.Stop ignores ctx) | RS-CR-01, RS-ME-10 |
| PY-HI-11 | `RateLimiter` `threading.Lock` + async | F-076 (lock through Submit) | RS-HI-06 |
| PY-HI-12 | `gather` w/o `return_exceptions=True` | F-008..F-010 | RS-HI-08 |
| PY-HI-13 | sync I/O in `async def` | F-106 (`time.Sleep` ignores ctx) | RS-ME-08 (`block_in_place`) |
| PY-HI-14 | JWT no leeway, no HMAC length, exposes decode_unverified | F-036, F-041 | RS-CR-02 |
| PY-HI-15 | `ARGON2` is actually scrypt | no parity (gokit doesn't ship pwd hash) | rskit ships Argon2id (win) |
| PY-HI-16 | CI missing pip-audit/bandit/SBOM/signing | F-007 | RS-ME-19 |
| PY-HI-17 | pip-audit can't complete | F-006 (govulncheck not gating) | RS-LO-09 (cargo-audit/deny dup) |
| PY-HI-18 | TLSConfig.is_enabled excludes min_version | F-040 | RS-ME-16 (no TLS knobs) |
| PY-HI-19 | `add_insecure_port` only — no TLS | F-040 (TLS MinVersion bug) | RS-CR-03 (gRPC plaintext) |
| PY-HI-20 | OIDC refresh leaks token-endpoint body | F-035 (cause flatten leak) | RS-LO-03 |
| PY-HI-21 | `_TYPE_BASE_URI` global | F-031 | RS-ME-13 |
| PY-HI-22 | `AppError.__str__` leaks cause | F-035 | RS-HI-15 (related: `!Clone`) |
| PY-HI-23 | No `Wrap` classifier | F-034 | RS-HI-14 |
| PY-HI-24 | `setup_*` mutate OTel globals, no shutdown | F-043 | RS-HI-17 |
| PY-HI-25 | `setup_tracing` no exporter wired | F-045 (init/shutdown ignore ctx) | RS-HI-17 |
| PY-HI-26 | No global `TextMapPropagator` | F-108 (no span around OIDC discovery) | RS-HI-17 |
| PY-HI-27 | mypy broken in CI (TS-01 dup) | F-024 | RS-HI-27 |
| PY-HI-28 | `pytest-xdist` not installed | F-055 (Windows no `-race`) | RS-HI-32 |
| PY-HI-29 | Coverage `fail_under = 60` | F-017 | RS-HI-19 |
| PY-HI-30 | No integration test separation | F-019 | RS-HI-22 |
| PY-HI-31 | 90 wall-clock sites; no freezegun | F-017-adjacent (TS-05) | RS-HI-23 |
| PY-HI-32 | No property-based or fuzz testing | F-018 | RS-HI-21 |
| PY-HI-33 | 10 failing `pykit-discovery` tests | F-011 (logger flake) | baseline-red (4 clippy + 23 fmt + 7 RUSTSEC) |
| PY-HI-34 | No perf benchmarking | F-020/F-021 | RS-HI-24/RS-HI-25 |
| PY-HI-35 | Zero `lru_cache`/`cache` | F-048 (zero `sync.Pool`) | RS-ME-32 (no object pooling) |
| PY-HI-36 | Ruff misses 11 rule families | F-023 | RS-HI-30 |
| PY-HI-37 | 271 `Any` in 50 unchecked pkgs | no parity | no parity |
| PY-HI-38 | mypy strict scope 5 of 55 (LT-04 dup) | F-024 | RS-HI-27 |
| PY-HI-39 | 12 mutable `@v4`/`@v5` refs | F-004 | RS-CR-05 |
| PY-HI-40 | No build matrix | F-053 | RS-HI-31 |
| PY-HI-41 | Coverage XML never uploaded | F-054 | RS-HI-19 |
| PY-HI-42 | mypy `--strict` 5 of 55 in CI | F-017 | RS-HI-27 |
| PY-HI-43 | No `uv lock --check` gate | F-061 | RS-HI-38 |
| PY-HI-44 | No fuzz job (atheris) | F-018, F-095 | RS-HI-21 |
| PY-HI-45 | No pre-commit | F-070 | RS-HI-47 |
| PY-HI-46 | No `.editorconfig` / `.gitattributes` | (gokit has) | RS-HI-45/46 |
| PY-HI-47 | python-version not pinned | F-001 (Go 1.26.0 + no toolchain) | RS-HI-28 |
| PY-HI-48 | hatch metadata incomplete | no parity (Go has no equivalent) | RS-HI-50 (no public-api baselines) |
| PY-HI-49 | `[project.optional-dependencies]` missing | F-060 (34-module split unjustified) | no parity |
| PY-HI-50 | No SECURITY.md | (gokit has) | RS-CR-08 |
| PY-HI-51 | README cites missing CODE_OF_CONDUCT | F-066 (release-process.md "(when present)" missing) | no parity |
| PY-HI-52 | No MAINTAINERS / GOVERNANCE / CODEOWNERS | F-029 (bus factor) | RS-CR-11, RS-HI-43, RS-HI-44 |
| PY-HI-53 | No per-package READMEs / no docs site | F-063 (thin runnable examples) | RS-HI-41, RS-HI-42 |
| PY-HI-54 | All `0.1.0`; no SemVer policy | F-028 | RS-HI-49 |
| PY-HI-55 | No tag protection / signed tags / release notes | F-068 | RS-HI-49 |
| PY-HI-56 | No SBOM / no Sigstore | F-007, F-025 | RS-ME-19 |
| PY-HI-57 | No deprecation policy | F-028 | RS-HI-50 |
| PY-HI-58 | No ISSUE_TEMPLATE / PR template | F-100 | RS-LO-24 |
| PY-HI-59 | No CHANGELOG.md | F-027 | RS-LO-23 |
| PY-HI-60 | No CONTRIBUTING.md | (gokit has) | (rskit lacks: RS-LO-22) |

---

<a id="sec-7"></a>
## 7. Notes on Comparison Asymmetries

1. **pykit `pykit-server` is gRPC-only.** Every "HTTP server / HTTP middleware / `/healthz` / Auth middleware / CORS / CSRF / OIDC verifier / JWKS cache" parity row above flags pykit with **N/A or ❌** rather than a pykit defect that "matches" gokit/rskit. The remediation in §5(d) creates these capabilities from scratch in pykit; there is no parallel "fix what exists" item.
2. **Go has no `unsafe impl Send/Sync` / no UB.** RS-HI-03 and RS-HI-26 have **no parity** in gokit; pykit's nearest-neighbour is the GIL-vs-asyncio mixing in PY-HI-11.
3. **Python has no MSRV semantics like Rust/Go**, so RS-HI-28's "1.91 vs 1.85" doesn't quite match PY-HI-47's "some pkgs allow 3.11, some 3.13"; the *spirit* (single language-version contract) parallels.
4. **gokit ships `SECURITY.md`/`MAINTAINERS.md`/`GOVERNANCE.md`/`CHANGELOG.md`**; for those, gokit is the *win*, not the defect site (see §3.1 #7-#8).
5. **rskit `rskit-bench` and pykit `pykit-bench` are both ML-evaluation harnesses misnamed as performance benches** (RS-ME-28, PY-LO-10) — same anti-pattern, with gokit's `bench/` directory carrying the same misnomer (F-081). All three should rename to `*-evals` and create real per-package benches.
6. **Coverage thresholds are differently broken across siblings.** gokit has no per-package floor at all (F-017); rskit has no coverage gate at all (RS-HI-19); pykit has a gate at 60% while measured is 90.81% (PY-HI-29). Lockstep target: ≥80% per-package, ≥85% workspace.
7. **The "registry" smell is the single highest-confidence cross-sibling root cause**, with 6+5+3 implementations and 5+5+3 different policies. §5(b) `core-v1` should be the first epic landed.

---

*End of cross-sibling matrix. This document is the input to Phase E (issue creation) — every Critical and High finding above gets a `Sibling: …` backlink line drawn from §6.*
