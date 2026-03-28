# rskit — Implementation Roadmap

Complete specification for bringing rskit to full parity with gokit.
Each item is self-contained: types, public API, dependencies, and implementation notes.

---

## Table of Contents

1. [Status Snapshot](#1-status-snapshot)
2. [Phase 1 — Foundation Gaps (existing crates)](#2-phase-1--foundation-gaps-existing-crates)
   - 2.1 [rskit-errors — ErrorResponse type](#21-rskit-errors--errorresponse-type)
   - 2.2 [rskit-logging — Global logger & component tagging](#22-rskit-logging--global-logger--component-tagging)
   - 2.3 [rskit-resilience — State-change & retry callbacks](#23-rskit-resilience--state-change--retry-callbacks)
   - 2.4 [rskit-bootstrap — `run_task`, concurrent startup, lazy components](#24-rskit-bootstrap--run_task-concurrent-startup-lazy-components)
3. [Phase 2 — New Core Crates](#3-phase-2--new-core-crates)
   - 3.1 [rskit-validation](#31-rskit-validation)
   - 3.2 [rskit-http](#32-rskit-http)
   - 3.3 [rskit-di](#33-rskit-di)
   - 3.4 [rskit-auth](#34-rskit-auth)
4. [Phase 3 — Infrastructure Adapters](#4-phase-3--infrastructure-adapters)
   - 4.1 [rskit-database](#41-rskit-database)
   - 4.2 [rskit-cache](#42-rskit-cache)
   - 4.3 [rskit-messaging](#43-rskit-messaging)
5. [Phase 4 — Platform Crates](#5-phase-4--platform-crates)
   - 5.1 [rskit-observability](#51-rskit-observability)
   - 5.2 [rskit-authz](#52-rskit-authz)
   - 5.3 [rskit-discovery](#53-rskit-discovery)
   - 5.4 [rskit-testutil](#54-rskit-testutil)
6. [Phase 5 — Specialist Crates](#5-phase-5--specialist-crates)
   - 6.1 [rskit-sse](#61-rskit-sse)
   - 6.2 [rskit-dag](#62-rskit-dag)
   - 6.3 [rskit-llm](#63-rskit-llm)
7. [Workspace Changes](#7-workspace-changes)
8. [Dependency Reference](#8-dependency-reference)
9. [Implementation Order](#9-implementation-order)

---

## 1. Status Snapshot

| Crate / Domain | gokit | rskit | Status |
|---|---|---|---|
| `errors` | ✓ | ✓ | Complete — minor gap: `ErrorResponse` |
| `config` | ✓ | ✓ | Complete |
| `logging` | ✓ | ✓ | Partial — missing global logger, component tagging |
| `validation` | ✓ | — | **Missing** (new crate) |
| `resilience` | ✓ | ✓ | Partial — missing callbacks |
| `bootstrap` | ✓ | ✓ | Partial — missing `run_task`, concurrent startup |
| `provider` | ✓ | ✓ | Complete (rskit superior) |
| `pipeline` | ✓ | ✓ | Complete |
| `worker` | ✓ | ✓ | Complete |
| `server` (gRPC) | ✓ | ✓ | Complete |
| `http` (REST) | ✓ | — | **Missing** (new crate) |
| `di` | ✓ | — | **Missing** (new crate) |
| `auth` | ✓ | — | **Missing** (new crate) |
| `database` | ✓ | — | **Missing** (new crate) |
| `cache` | ✓ | — | **Missing** (new crate) |
| `messaging` | ✓ | — | **Missing** (new crate) |
| `observability` | ✓ | — | **Missing** (new crate) |
| `authz` | ✓ | — | **Missing** (new crate) |
| `discovery` | ✓ | — | **Missing** (new crate) |
| `testutil` | ✓ | — | **Missing** (new crate) |
| `sse` | ✓ | — | **Missing** (new crate) |
| `dag` | ✓ | — | **Missing** (new crate) |
| `llm` | ✓ | — | **Missing** (new crate) |

**Existing crates:** 10 | **New crates:** 13 | **Enhancements:** 4

---

## 2. Phase 1 — Foundation Gaps (existing crates)

### 2.1 `rskit-errors` — `ErrorResponse` type

**What gokit has:** RFC 7807–style `ErrorResponse` / `ErrorBody` returned from HTTP handlers, with `type`, `title`, `status`, `detail`, `instance` fields — standard machine-readable error envelope.

**What to add:**

```
crates/rskit-errors/src/response.rs
```

```rust
/// RFC 7807 Problem Details response body.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ErrorResponse {
    /// URI reference identifying the problem type.
    #[serde(rename = "type")]
    pub error_type: String,
    /// Short human-readable summary.
    pub title: String,
    /// HTTP status code.
    pub status: u16,
    /// Human-readable explanation of this specific occurrence.
    pub detail: String,
    /// URI reference identifying this specific occurrence (optional).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub instance: Option<String>,
    /// Additional context key-value pairs.
    #[serde(skip_serializing_if = "HashMap::is_empty")]
    pub extensions: HashMap<String, String>,
}

impl ErrorResponse {
    pub fn from_app_error(err: &AppError) -> Self { ... }
}

impl From<&AppError> for ErrorResponse { ... }
impl From<AppError>  for ErrorResponse { ... }
```

**`lib.rs` export:** `pub use response::ErrorResponse;`

---

### 2.2 `rskit-logging` — Global logger & component tagging

**What gokit has:** `GetGlobalLogger()`, `SetGlobalLogger()`, per-component tagging via `WithComponent("name")`, service-name field injected on every line, request/correlation ID enrichment helpers.

**What to add:**

```
crates/rskit-logging/src/context.rs   ← span field helpers
crates/rskit-logging/src/global.rs    ← global subscriber guard
```

**`context.rs`:**

```rust
/// Returns a new Span with a `component` field set.
/// Usage: let _span = component_span("auth-service").entered();
pub fn component_span(name: &'static str) -> tracing::Span;

/// Returns a new Span enriched with HTTP request metadata.
pub fn request_span(method: &str, path: &str, request_id: &str) -> tracing::Span;

/// Injects a correlation ID into the current span.
pub fn set_correlation_id(id: &str);

/// Injects a user ID into the current span.
pub fn set_user_id(id: &str);

/// Injects a trace ID into the current span.
pub fn set_trace_id(id: &str);
```

**`global.rs`:**

```rust
/// Initialise a global default subscriber that all `tracing::` calls
/// fall back to when no local subscriber is set.
/// Safe to call once at program start. Subsequent calls are no-ops.
pub fn init_global(cfg: &LoggingConfig) -> LoggingGuard;

/// Returns `true` if a global subscriber has been initialised.
pub fn is_global_init() -> bool;
```

**`LoggingConfig` additions:**

```rust
pub struct LoggingConfig {
    pub level:        String,
    pub format:       LogFormat,
    pub service_name: Option<String>,   // ← NEW: injected on every span
    pub output:       LogOutput,        // ← NEW: Stdout | Stderr | File(path)
    pub with_caller:  bool,             // ← NEW: include file:line
}
```

---

### 2.3 `rskit-resilience` — State-change & retry callbacks

**What gokit has:** `OnStateChange(from, to State)` on circuit breaker; `OnRetry(attempt, err)` on retry; `OnReject` / `OnAcquire` / `OnRelease` on bulkhead.

**What to add in existing files:**

**`circuit_breaker.rs`:**

```rust
pub struct CbConfig {
    // existing fields ...
    /// Called whenever the circuit transitions between states.
    pub on_state_change: Option<Arc<dyn Fn(CbState, CbState) + Send + Sync>>,
}

impl CbConfig {
    #[must_use]
    pub fn with_on_state_change(
        mut self,
        f: impl Fn(CbState, CbState) + Send + Sync + 'static,
    ) -> Self;
}
```

**`retry.rs`:**

```rust
pub struct RetryPolicy {
    // existing fields ...
    /// Called after each failed attempt before the next backoff sleep.
    pub on_retry: Option<Arc<dyn Fn(u32, &AppError) + Send + Sync>>,
    /// Predicate: return `false` to stop retrying immediately.
    pub retry_if: Option<Arc<dyn Fn(&AppError) -> bool + Send + Sync>>,
}

impl RetryPolicy {
    #[must_use]
    pub fn with_on_retry(mut self, f: impl Fn(u32, &AppError) + Send + Sync + 'static) -> Self;

    #[must_use]
    pub fn with_retry_if(mut self, f: impl Fn(&AppError) -> bool + Send + Sync + 'static) -> Self;
}
```

**`bulkhead.rs`:**

```rust
pub struct BulkheadConfig {
    // existing fields ...
    pub on_reject:  Option<Arc<dyn Fn() + Send + Sync>>,
    pub on_acquire: Option<Arc<dyn Fn() + Send + Sync>>,
    pub on_release: Option<Arc<dyn Fn() + Send + Sync>>,
}
```

---

### 2.4 `rskit-bootstrap` — `run_task`, concurrent startup, lazy components

**What gokit has:** `RunTask()` drives a finite async job with the same config/DI/logging setup as a full service but exits on completion instead of waiting for SIGTERM. Also: components started concurrently up to a configurable concurrency limit.

**`app.rs` additions:**

```rust
impl<C: AppConfig> AppBuilder<Configured, C> {
    /// Drive a finite async task instead of running as a persistent service.
    /// Exits cleanly when `task` returns. Still sets up config, logging, and
    /// cancel token (fires on SIGINT/SIGTERM or task completion).
    pub async fn run_task<F, Fut>(self, task: F) -> AppResult<()>
    where
        F: FnOnce(C, CancellationToken) -> Fut,
        Fut: Future<Output = AppResult<()>>;
}
```

**`registry.rs` additions:**

```rust
pub struct RegistryConfig {
    /// Maximum number of components to start in parallel (default: sequential).
    pub concurrency: usize,
    /// Per-component start timeout.
    pub start_timeout: Duration,
    /// Per-component stop timeout.
    pub stop_timeout: Duration,
}

impl Registry {
    pub fn with_config(cfg: RegistryConfig) -> Self;

    /// Starts all components concurrently up to `cfg.concurrency`.
    /// Order is still deterministic (insertion order batched by concurrency).
    pub async fn start_all_concurrent(
        &self,
        cancel: CancellationToken,
    ) -> AppResult<()>;
}
```

**Lazy component wrapper:**

```rust
/// Wraps a component factory: the inner component is not constructed
/// until `start()` is first called.
pub struct LazyComponent<F> {
    name: &'static str,
    factory: F,
    inner: parking_lot::Mutex<Option<Arc<dyn Component>>>,
}

impl<F: Fn() -> Arc<dyn Component> + Send + Sync> LazyComponent<F> {
    pub fn new(name: &'static str, factory: F) -> Self;
}

// Implements Component by delegating to the lazily-built inner.
```

---

## 3. Phase 2 — New Core Crates

### 3.1 `rskit-validation`

**What gokit has:** Fluent field-level validator: `validator.Required("name", value).Email("email", value).MaxLength("bio", value, 500).Validate()` → returns `AppError` with all field errors collected.

**Location:** `crates/rskit-validation/`

**Key dependencies:**
```toml
rskit-errors = { path = "../rskit-errors" }
validator    = { workspace = true }   # derive macro kept for struct-level
regex        = "1"
```

**Public API:**

```rust
// ── Field-level validator ─────────────────────────────────────────────────

/// Fluent builder that collects field errors and converts to AppError.
pub struct Validator {
    errors: Vec<FieldError>,
}

#[derive(Debug, Clone)]
pub struct FieldError {
    pub field:   String,
    pub message: String,
}

impl Validator {
    pub fn new() -> Self;

    // String checks
    #[must_use] pub fn required(self, field: &str, value: &str) -> Self;
    #[must_use] pub fn min_length(self, field: &str, value: &str, min: usize) -> Self;
    #[must_use] pub fn max_length(self, field: &str, value: &str, max: usize) -> Self;
    #[must_use] pub fn email(self, field: &str, value: &str) -> Self;
    #[must_use] pub fn url(self, field: &str, value: &str) -> Self;
    #[must_use] pub fn pattern(self, field: &str, value: &str, re: &str) -> Self;

    // UUID
    #[must_use] pub fn required_uuid(self, field: &str, value: &str) -> Self;
    #[must_use] pub fn optional_uuid(self, field: &str, value: Option<&str>) -> Self;

    // Numeric
    #[must_use] pub fn in_range<T: PartialOrd + Display>(
        self, field: &str, value: T, min: T, max: T,
    ) -> Self;

    // Time
    #[must_use] pub fn before(self, field: &str, value: &str, deadline: &str) -> Self;
    #[must_use] pub fn after(self, field: &str, value: &str, floor: &str) -> Self;

    // Enum membership
    #[must_use] pub fn one_of<T: PartialEq + Display>(
        self, field: &str, value: &T, allowed: &[T],
    ) -> Self;

    // Composition
    #[must_use] pub fn custom(self, field: &str, check: bool, message: &str) -> Self;

    // Terminal
    pub fn has_errors(&self) -> bool;
    pub fn errors(&self) -> &[FieldError];
    pub fn validate(self) -> AppResult<()>;
}

// ── Convenience free functions ────────────────────────────────────────────

pub fn validate_email(value: &str) -> bool;
pub fn validate_url(value: &str) -> bool;
pub fn validate_uuid(value: &str) -> bool;
```

**`src/lib.rs`:** `pub use validator::{Validate, ValidationErrors};` re-exported for derive macro users.

---

### 3.2 `rskit-http`

**What gokit has:** HTTP server wrapping `net/http` with Gin router, h2c (HTTP/2 cleartext), handler mounting, request context enrichment, structured error middleware, CORS, graceful shutdown.

**Location:** `crates/rskit-http/`

**Chosen Rust equivalent:** **axum** (Tower-native, composes with rskit-resilience layers, strong ecosystem).

**Key dependencies:**
```toml
axum         = { version = "0.8", features = ["http2"] }
tower        = { workspace = true }
tower-http   = { version = "0.6", features = ["cors", "trace", "request-id", "timeout"] }
hyper        = { version = "1", features = ["full"] }
hyper-util   = { version = "0.1", features = ["tokio"] }
rskit-errors    = { path = "../rskit-errors" }
rskit-bootstrap = { path = "../rskit-bootstrap" }
rskit-logging   = { path = "../rskit-logging" }
```

**Public API:**

```rust
// ── Config ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize, Validate)]
pub struct HttpServerConfig {
    #[validate(length(min = 1))]
    pub host:          String,           // default: "0.0.0.0"
    #[validate(range(min = 1, max = 65535))]
    pub port:          u16,              // default: 8080
    pub read_timeout:  Duration,         // default: 30s
    pub write_timeout: Duration,         // default: 30s
    pub idle_timeout:  Duration,         // default: 60s
    pub enable_h2c:    bool,             // default: true
    pub cors:          Option<CorsConfig>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CorsConfig {
    pub allowed_origins:  Vec<String>,
    pub allowed_methods:  Vec<String>,
    pub allowed_headers:  Vec<String>,
    pub allow_credentials: bool,
    pub max_age:          Duration,
}

// ── Builder ───────────────────────────────────────────────────────────────

pub struct HttpServerBuilder {
    config: HttpServerConfig,
    cancel: CancellationToken,
    router: axum::Router,
}

impl HttpServerBuilder {
    pub fn new(config: HttpServerConfig, cancel: CancellationToken) -> Self;

    /// Merge an axum Router (routes, middleware) into the server.
    #[must_use] pub fn with_router(self, router: axum::Router) -> Self;

    /// Mount a raw Tower service at a path prefix (for gRPC, Connect-Go, etc.).
    #[must_use] pub fn mount<S>(self, path: &str, service: S) -> Self
    where S: tower::Service<...> + Clone + Send + 'static;

    /// Apply a Tower middleware layer to all routes.
    #[must_use] pub fn with_layer<L: tower::Layer<...>>(self, layer: L) -> Self;

    /// Add CORS from config (wraps tower-http CorsLayer).
    #[must_use] pub fn with_cors(self) -> Self;

    /// Add automatic request ID injection (X-Request-Id header).
    #[must_use] pub fn with_request_id(self) -> Self;

    /// Add automatic tracing span per request.
    #[must_use] pub fn with_tracing(self) -> Self;

    pub fn build(self) -> AppResult<HttpServer>;
}

// ── Server (implements Component) ────────────────────────────────────────

pub struct HttpServer { ... }

#[async_trait]
impl Component for HttpServer {
    async fn start(&self, cancel: CancellationToken) -> AppResult<()>;
    async fn stop(&self)  -> AppResult<()>;
    async fn health(&self) -> Health;
}

// ── Error handling middleware ──────────────────────────────────────────────

/// axum IntoResponse for AppError → JSON ErrorResponse body.
impl axum::response::IntoResponse for AppError { ... }

/// Tower layer that catches panics and converts them to 500 AppErrors.
pub struct ErrorHandlerLayer;

// ── Extractors ────────────────────────────────────────────────────────────

/// axum extractor that reads X-Request-Id header.
pub struct RequestId(pub String);

/// axum extractor that reads X-Correlation-Id header.
pub struct CorrelationId(pub String);

// ── Router helpers ────────────────────────────────────────────────────────

/// Adds a `/health` endpoint returning JSON HealthStatus.
pub fn health_router(registry: Arc<Registry>) -> axum::Router;
```

**Design notes:**
- h2c is handled by `hyper-util::server::conn::auto::Builder` — HTTP/1.1 and HTTP/2 on the same port without TLS.
- gRPC over h2c works by mounting the tonic `GrpcServer` at `/` via `mount("/", grpc_service)`.
- All rskit-resilience Tower layers compose directly with axum routes.

---

### 3.3 `rskit-di`

**What gokit has:** `Container` interface with `Register()`, `RegisterLazy()`, `RegisterEager()`, `RegisterSingleton()`, `Resolve()`, `Close()`. Circular dependency detection. Per-component retry and circuit breaker.

**Design note for Rust:** Go's DI uses `interface{}` + runtime reflection. In Rust, DI containers have a fundamentally different trade-off: the type system can express much of DI at compile time. The idiomatic approach is **typed owned construction** (what rskit already uses). However, for large application graphs with many optional/lazy components, a runtime container is useful.

**Strategy:** Lightweight `Arc`-based runtime container keyed by `TypeId`. No proc-macro magic. Lazy init with `OnceLock`.

**Location:** `crates/rskit-di/`

**Key dependencies:**
```toml
rskit-errors    = { path = "../rskit-errors" }
rskit-resilience = { path = "../rskit-resilience" }
parking_lot     = { workspace = true }
```

**Public API:**

```rust
// ── Container ─────────────────────────────────────────────────────────────

/// Thread-safe runtime DI container.
pub struct Container {
    registrations: parking_lot::RwLock<HashMap<TypeId, Registration>>,
}

enum Registration {
    Eager(Arc<dyn Any + Send + Sync>),
    Lazy(Arc<dyn Fn() -> AppResult<Arc<dyn Any + Send + Sync>> + Send + Sync>),
    Singleton {
        factory: Arc<dyn Fn() -> AppResult<Arc<dyn Any + Send + Sync>> + Send + Sync>,
        instance: OnceLock<Arc<dyn Any + Send + Sync>>,
    },
}

impl Container {
    pub fn new() -> Self;

    /// Register a pre-built value (equivalent to gokit RegisterEager).
    pub fn register<T: Send + Sync + 'static>(&self, value: Arc<T>);

    /// Register a factory called fresh on every resolve (equivalent to RegisterLazy).
    pub fn register_factory<T, F>(&self, factory: F)
    where
        T: Send + Sync + 'static,
        F: Fn() -> AppResult<Arc<T>> + Send + Sync + 'static;

    /// Register a singleton factory — called once, result cached (equivalent to RegisterSingleton).
    pub fn register_singleton<T, F>(&self, factory: F)
    where
        T: Send + Sync + 'static,
        F: Fn() -> AppResult<Arc<T>> + Send + Sync + 'static;

    /// Resolve a registered type.
    pub fn resolve<T: Send + Sync + 'static>(&self) -> AppResult<Arc<T>>;

    /// Returns true if `T` has been registered.
    pub fn is_registered<T: 'static>(&self) -> bool;

    /// Call `close()` on all registered values that implement `Closeable`.
    pub async fn close(&self) -> AppResult<()>;
}

// ── Closeable ─────────────────────────────────────────────────────────────

#[async_trait]
pub trait Closeable: Send + Sync {
    async fn close(&self) -> AppResult<()>;
}

// ── Resilient resolver ─────────────────────────────────────────────────────

/// Resolves a dependency with retry + circuit breaker (mirrors gokit's
/// per-component resilience in DI).
pub struct ResilientResolver {
    container: Arc<Container>,
    retry:     RetryPolicy,
    cb:        CircuitBreaker,
}

impl ResilientResolver {
    pub fn new(container: Arc<Container>, retry: RetryPolicy, cb: CircuitBreaker) -> Self;

    pub async fn resolve<T: Send + Sync + 'static>(&self) -> AppResult<Arc<T>>;
}
```

---

### 3.4 `rskit-auth`

**What gokit has:** `TokenValidator` / `TokenGenerator` traits; `jwt` sub-package (sign/verify with HMAC or RSA); `oidc` sub-package (JWKS, introspection, state); `password` sub-package (bcrypt hash, reset token); `authctx` (store/retrieve claims from `context.Context`).

**Location:** `crates/rskit-auth/`

**Sub-modules:**

```
src/
  lib.rs
  traits.rs       ← TokenValidator, TokenGenerator
  jwt/
    mod.rs
    service.rs    ← JwtService<C>
    config.rs     ← JwtConfig
  oidc/
    mod.rs
    provider.rs   ← OidcProvider
    config.rs     ← OidcConfig
  password/
    mod.rs
    hasher.rs     ← PasswordHasher
    reset.rs      ← ResetTokenGenerator
  context.rs      ← Claims stored in task-local / request-extension
```

**Key dependencies:**
```toml
jsonwebtoken  = "9"
argon2        = "0.5"
bcrypt        = "0.15"
reqwest       = { version = "0.12", features = ["json", "rustls-tls"], default-features = false }
serde         = { workspace = true }
rskit-errors  = { path = "../rskit-errors" }
```

**Public API:**

```rust
// ── Core traits ───────────────────────────────────────────────────────────

#[async_trait]
pub trait TokenValidator<C>: Send + Sync {
    async fn validate(&self, token: &str) -> AppResult<C>;
}

#[async_trait]
pub trait TokenGenerator<C>: Send + Sync {
    async fn generate(&self, claims: &C) -> AppResult<String>;
}

// ── JWT ────────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct JwtConfig {
    pub secret:    String,
    pub algorithm: Algorithm,   // HS256 | HS384 | HS512 | RS256 | RS384 | RS512
    pub ttl:       Duration,
    pub issuer:    Option<String>,
    pub audience:  Option<Vec<String>>,
}

pub struct JwtService<C> {
    config: JwtConfig,
    _claims: PhantomData<C>,
}

impl<C: Serialize + DeserializeOwned + Send + Sync> JwtService<C> {
    pub fn new(config: JwtConfig) -> Self;
}

impl<C: Serialize + DeserializeOwned + Send + Sync> TokenGenerator<C> for JwtService<C> { ... }
impl<C: Serialize + DeserializeOwned + Send + Sync> TokenValidator<C> for JwtService<C> { ... }

// ── OIDC ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct OidcConfig {
    pub issuer:          String,
    pub client_id:       String,
    pub client_secret:   Option<String>,
    pub jwks_uri:        Option<String>,   // auto-discovered if None
    pub audience:        Vec<String>,
    pub jwks_refresh:    Duration,         // default: 1h
}

pub struct OidcProvider {
    config: OidcConfig,
    jwks:   Arc<tokio::sync::RwLock<JwkSet>>,
}

impl OidcProvider {
    pub async fn new(config: OidcConfig) -> AppResult<Self>;

    /// Validate a Bearer token against the JWKS.
    pub async fn validate<C: DeserializeOwned>(&self, token: &str) -> AppResult<C>;

    /// Force-refresh the JWKS cache.
    pub async fn refresh_jwks(&self) -> AppResult<()>;
}

// ── Password ──────────────────────────────────────────────────────────────

pub struct PasswordHasher {
    algorithm: HashAlgorithm,   // Argon2id (default) | Bcrypt
}

pub enum HashAlgorithm { Argon2id, Bcrypt }

impl PasswordHasher {
    pub fn new(algorithm: HashAlgorithm) -> Self;

    pub fn hash(&self, password: &str)             -> AppResult<String>;
    pub fn verify(&self, password: &str, hash: &str) -> AppResult<bool>;
}

/// Generates short-lived opaque reset tokens (random bytes, base64-URL).
pub struct ResetTokenGenerator {
    ttl: Duration,
}

impl ResetTokenGenerator {
    pub fn new(ttl: Duration) -> Self;
    pub fn generate(&self) -> (String, DateTime<Utc>);  // (token, expires_at)
}

// ── Request context ────────────────────────────────────────────────────────

/// Store claims in an axum request extension.
pub struct AuthClaims<C>(pub C);

/// axum extractor that reads `AuthClaims<C>` from extensions.
/// Returns 401 AppError if not present.
pub struct RequireAuth<C>(pub C);

// (For non-axum use, a task_local! or thread_local! equivalent is provided.)
```

---

## 4. Phase 3 — Infrastructure Adapters

### 4.1 `rskit-database`

**What gokit has:** GORM-based DB wrapper, connection pool, slow-query logger, repository pattern (generic CRUD), MySQL/Postgres/SQLite adapters, `Component` lifecycle.

**Location:** `crates/rskit-database/`

**Chosen Rust equivalent:** **sqlx** (compile-time query checking, async-native, multi-driver).

**Key dependencies:**
```toml
sqlx         = { version = "0.8", features = ["runtime-tokio", "tls-rustls", "postgres", "mysql", "sqlite", "uuid", "chrono", "json"] }
rskit-errors    = { path = "../rskit-errors" }
rskit-bootstrap = { path = "../rskit-bootstrap" }
rskit-resilience = { path = "../rskit-resilience" }
```

**Public API:**

```rust
// ── Config ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize, Validate)]
pub struct DatabaseConfig {
    pub driver:            DbDriver,       // Postgres | Mysql | Sqlite
    pub host:              String,
    pub port:              u16,
    pub user:              String,
    pub password:          String,
    pub database:          String,
    pub max_connections:   u32,            // default: 10
    pub min_connections:   u32,            // default: 1
    pub connect_timeout:   Duration,       // default: 30s
    pub idle_timeout:      Option<Duration>,
    pub max_lifetime:      Option<Duration>,
    pub slow_query_threshold: Duration,    // default: 1s (logs warn)
    pub ssl_mode:          SslMode,        // Disable | Prefer | Require
}

pub enum DbDriver { Postgres, Mysql, Sqlite }
pub enum SslMode  { Disable, Prefer, Require }

// ── Database (implements Component) ──────────────────────────────────────

pub struct Database {
    pool: sqlx::AnyPool,
    config: DatabaseConfig,
}

impl Database {
    pub async fn new(config: DatabaseConfig) -> AppResult<Self>;

    /// Raw pool access for migrations or custom queries.
    pub fn pool(&self) -> &sqlx::AnyPool;

    /// Execute a query with automatic slow-query logging.
    pub async fn execute(&self, query: &str) -> AppResult<sqlx::postgres::PgQueryResult>;
}

#[async_trait]
impl Component for Database { ... }

// ── Repository trait ──────────────────────────────────────────────────────

/// Generic CRUD repository backed by a `Database`.
#[async_trait]
pub trait Repository<T, ID>: Send + Sync
where
    T:  Send + Sync,
    ID: Send + Sync,
{
    async fn find_by_id(&self, id: &ID)       -> AppResult<Option<T>>;
    async fn find_all(&self, opts: FindOpts)   -> AppResult<Vec<T>>;
    async fn find_first(&self, opts: FindOpts) -> AppResult<Option<T>>;
    async fn count(&self, opts: FindOpts)      -> AppResult<i64>;
    async fn exists(&self, id: &ID)            -> AppResult<bool>;

    async fn create(&self, entity: &T)         -> AppResult<T>;
    async fn update(&self, entity: &T)         -> AppResult<T>;
    async fn delete(&self, id: &ID)            -> AppResult<()>;
    async fn upsert(&self, entity: &T)         -> AppResult<T>;
}

/// Query options for find operations.
#[derive(Debug, Default)]
pub struct FindOpts {
    pub limit:    Option<i64>,
    pub offset:   Option<i64>,
    pub order_by: Vec<String>,       // e.g. ["created_at DESC"]
    pub filters:  Vec<(String, serde_json::Value)>,  // e.g. [("status", "active")]
}

impl FindOpts {
    #[must_use] pub fn with_limit(self, n: i64)        -> Self;
    #[must_use] pub fn with_offset(self, n: i64)       -> Self;
    #[must_use] pub fn order_by(self, col: &str)       -> Self;
    #[must_use] pub fn filter(self, col: &str, val: impl Into<serde_json::Value>) -> Self;
}

// ── Base implementation helper ────────────────────────────────────────────

/// Concrete implementation of Repository for sqlx.
/// Embed this in your own repository structs.
pub struct SqlRepository<T> {
    db:         Arc<Database>,
    table_name: &'static str,
    _marker:    PhantomData<T>,
}
```

---

### 4.2 `rskit-cache`

**What gokit has:** Redis client (go-redis), connection pool, health check, typed key-value `TypedStore<T>` with JSON serialization, string/hash/list/set/sorted-set ops, TTL helpers, `Component` lifecycle.

**Location:** `crates/rskit-cache/`

**Key dependencies:**
```toml
redis        = { version = "0.27", features = ["tokio-comp", "connection-manager", "json"] }
rskit-errors    = { path = "../rskit-errors" }
rskit-bootstrap = { path = "../rskit-bootstrap" }
serde         = { workspace = true }
serde_json    = { workspace = true }
```

**Public API:**

```rust
// ── Config ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize, Validate)]
pub struct RedisConfig {
    pub host:        String,
    pub port:        u16,            // default: 6379
    pub password:    Option<String>,
    pub database:    u8,             // default: 0
    pub pool_size:   u32,            // default: 10
    pub connect_timeout: Duration,
    pub key_prefix:  Option<String>, // prefixed on every key
}

// ── Client (implements Component) ─────────────────────────────────────────

pub struct RedisClient {
    manager: redis::aio::ConnectionManager,
    config:  RedisConfig,
}

impl RedisClient {
    pub async fn new(config: RedisConfig) -> AppResult<Self>;

    // String ops
    pub async fn get(&self, key: &str)                          -> AppResult<Option<String>>;
    pub async fn set(&self, key: &str, val: &str, ttl: Option<Duration>) -> AppResult<()>;
    pub async fn delete(&self, key: &str)                       -> AppResult<bool>;
    pub async fn exists(&self, key: &str)                       -> AppResult<bool>;
    pub async fn expire(&self, key: &str, ttl: Duration)        -> AppResult<bool>;
    pub async fn ttl(&self, key: &str)                          -> AppResult<Option<Duration>>;
    pub async fn incr(&self, key: &str, delta: i64)             -> AppResult<i64>;

    // Hash ops
    pub async fn hget(&self, key: &str, field: &str)            -> AppResult<Option<String>>;
    pub async fn hset(&self, key: &str, field: &str, val: &str) -> AppResult<()>;
    pub async fn hdel(&self, key: &str, field: &str)            -> AppResult<bool>;
    pub async fn hgetall(&self, key: &str)                      -> AppResult<HashMap<String, String>>;

    // List ops
    pub async fn lpush(&self, key: &str, vals: &[&str])         -> AppResult<i64>;
    pub async fn rpush(&self, key: &str, vals: &[&str])         -> AppResult<i64>;
    pub async fn lrange(&self, key: &str, start: i64, stop: i64) -> AppResult<Vec<String>>;
    pub async fn llen(&self, key: &str)                         -> AppResult<i64>;

    // Scan
    pub async fn scan(&self, pattern: &str) -> AppResult<Vec<String>>;

    // Pub/Sub
    pub async fn publish(&self, channel: &str, msg: &str) -> AppResult<()>;
    pub async fn subscribe(&self, channel: &str) -> AppResult<impl Stream<Item = String>>;
}

#[async_trait]
impl Component for RedisClient { ... }

// ── TypedStore — generic JSON-serialized store ────────────────────────────

pub struct TypedStore<T> {
    client:  Arc<RedisClient>,
    prefix:  String,
    _marker: PhantomData<T>,
}

impl<T: Serialize + DeserializeOwned + Send + Sync> TypedStore<T> {
    pub fn new(client: Arc<RedisClient>, prefix: impl Into<String>) -> Self;

    pub async fn get(&self, key: &str)                       -> AppResult<Option<T>>;
    pub async fn set(&self, key: &str, val: &T, ttl: Option<Duration>) -> AppResult<()>;
    pub async fn delete(&self, key: &str)                    -> AppResult<bool>;
    pub async fn exists(&self, key: &str)                    -> AppResult<bool>;
}
```

---

### 4.3 `rskit-messaging`

**What gokit has:** Kafka `Producer` and `Consumer` abstractions, message routing, partition selection, compression, `Component` lifecycle.

**Location:** `crates/rskit-messaging/`

**Strategy:** Abstract the broker behind traits so Kafka (rdkafka) and in-memory (for testing) are swappable.

**Key dependencies:**
```toml
rdkafka      = { version = "0.37", features = ["tokio"] }
rskit-errors    = { path = "../rskit-errors" }
rskit-bootstrap = { path = "../rskit-bootstrap" }
rskit-pipeline  = { path = "../rskit-pipeline" }
serde         = { workspace = true }
```

**Public API:**

```rust
// ── Message ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone)]
pub struct Message<T> {
    pub topic:     String,
    pub key:       Option<String>,
    pub payload:   T,
    pub headers:   HashMap<String, String>,
    pub timestamp: DateTime<Utc>,
    pub partition: Option<i32>,
    pub offset:    Option<i64>,
}

// ── Producer trait ────────────────────────────────────────────────────────

#[async_trait]
pub trait MessageProducer<T>: Send + Sync {
    async fn send(&self, msg: Message<T>) -> AppResult<()>;
    async fn send_batch(&self, msgs: Vec<Message<T>>) -> AppResult<()>;
    async fn flush(&self, timeout: Duration) -> AppResult<()>;
}

// ── Consumer trait ────────────────────────────────────────────────────────

#[async_trait]
pub trait MessageConsumer<T>: Send + Sync {
    /// Returns a stream of messages. Caller commits offsets by calling
    /// `handle.commit()` on success.
    async fn subscribe(
        &self,
        topics: &[&str],
    ) -> AppResult<impl Stream<Item = AppResult<MessageHandle<T>>>>;
}

pub struct MessageHandle<T> {
    pub message: Message<T>,
    commit_fn:   Box<dyn FnOnce() -> AppResult<()> + Send>,
}

impl<T> MessageHandle<T> {
    pub fn commit(self) -> AppResult<()>;
    pub fn reject(self) -> AppResult<()>;
}

// ── Kafka implementations ─────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct KafkaConfig {
    pub brokers:          Vec<String>,
    pub group_id:         Option<String>,
    pub compression:      Compression,   // None | Gzip | Snappy | Lz4 | Zstd
    pub auto_offset_reset: OffsetReset,  // Earliest | Latest
    pub session_timeout:  Duration,
    pub batch_size:       usize,
    pub linger_ms:        u64,
}

pub struct KafkaProducer<T> { ... }
pub struct KafkaConsumer<T> { ... }

impl<T: Serialize + Send + Sync> KafkaProducer<T> {
    pub fn new(config: KafkaConfig) -> AppResult<Self>;
}

impl<T: DeserializeOwned + Send + Sync> KafkaConsumer<T> {
    pub fn new(config: KafkaConfig) -> AppResult<Self>;
}

// Component impls for both Producer and Consumer.
```

---

## 5. Phase 4 — Platform Crates

### 5.1 `rskit-observability`

**What gokit has:** OpenTelemetry tracer (gRPC/OTLP exporter), OpenTelemetry meter (Prometheus exporter), context propagation helpers, health-check types.

**Location:** `crates/rskit-observability/`

**Key dependencies:**
```toml
opentelemetry       = { version = "0.27" }
opentelemetry_sdk   = { version = "0.27", features = ["rt-tokio"] }
opentelemetry-otlp  = { version = "0.27", features = ["grpc-tonic", "metrics"] }
opentelemetry-prometheus = { version = "0.27" }
prometheus          = { version = "0.13" }
tracing-opentelemetry = "0.28"
rskit-logging        = { path = "../rskit-logging" }
rskit-bootstrap      = { path = "../rskit-bootstrap" }
rskit-errors         = { path = "../rskit-errors" }
```

**Public API:**

```rust
// ── Tracer ────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct TracingConfig {
    pub service_name: String,
    pub endpoint:     String,          // OTLP gRPC endpoint, e.g. "http://localhost:4317"
    pub sample_rate:  f64,             // 0.0 – 1.0
    pub export_timeout: Duration,
}

/// Initialise an OpenTelemetry tracer and wire it into the tracing subscriber.
/// Returns a guard — drop to flush and shut down the exporter.
pub fn init_tracer(cfg: &TracingConfig) -> AppResult<TracerGuard>;

pub struct TracerGuard { ... }
impl Drop for TracerGuard {
    fn drop(&mut self) { opentelemetry::global::shutdown_tracer_provider(); }
}

// ── Metrics ───────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Deserialize)]
pub struct MetricsConfig {
    pub service_name:  String,
    pub export_interval: Duration,     // default: 30s
    pub prometheus_port: Option<u16>,  // if set, expose /metrics on this port
    pub otlp_endpoint:   Option<String>,
}

/// Initialise OpenTelemetry metrics with Prometheus + optional OTLP export.
pub fn init_metrics(cfg: &MetricsConfig) -> AppResult<MetricsHandle>;

pub struct MetricsHandle {
    meter: opentelemetry::metrics::Meter,
}

impl MetricsHandle {
    pub fn counter(&self, name: &str, description: &str) -> Counter<u64>;
    pub fn histogram(&self, name: &str, description: &str) -> Histogram<f64>;
    pub fn gauge(&self, name: &str, description: &str) -> ObservableGauge<f64>;
    pub fn up_down_counter(&self, name: &str, description: &str) -> UpDownCounter<i64>;
}

// ── Context propagation ───────────────────────────────────────────────────

/// Inject W3C trace context into an HTTP header map.
pub fn inject_trace_context(headers: &mut HeaderMap);

/// Extract W3C trace context from an HTTP header map.
pub fn extract_trace_context(headers: &HeaderMap) -> opentelemetry::Context;
```

---

### 5.2 `rskit-authz`

**What gokit has:** `Checker` interface (`Check(ctx, action, resource, claims) → bool`), `Matcher` for rule sets, RBAC/ABAC policy engine.

**Location:** `crates/rskit-authz/`

**Key dependencies:**
```toml
rskit-errors = { path = "../rskit-errors" }
rskit-auth   = { path = "../rskit-auth" }
serde        = { workspace = true }
```

**Public API:**

```rust
// ── Core traits ────────────────────────────────────────────────────────────

#[async_trait]
pub trait Checker: Send + Sync {
    /// Returns Ok(()) if the action is permitted, Err(AppError::Forbidden) otherwise.
    async fn check(
        &self,
        subject:  &str,   // e.g. user ID or role
        action:   &str,   // e.g. "read", "write", "delete"
        resource: &str,   // e.g. "documents", "documents:42"
    ) -> AppResult<()>;
}

// ── RBAC ──────────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Policy {
    pub subject:  String,
    pub action:   String,
    pub resource: String,
    pub effect:   Effect,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Effect { Allow, Deny }

pub struct RbacChecker {
    policies: Vec<Policy>,
}

impl RbacChecker {
    pub fn new(policies: Vec<Policy>) -> Self;
    pub fn add_policy(&mut self, policy: Policy);
    pub fn remove_policy(&mut self, subject: &str, action: &str, resource: &str);
}

#[async_trait]
impl Checker for RbacChecker { ... }

// ── ABAC (attribute-based, evaluates rules against claims map) ─────────────

pub struct AbacChecker {
    rules: Vec<Box<dyn AbacRule>>,
}

pub trait AbacRule: Send + Sync {
    fn evaluate(
        &self,
        claims:   &HashMap<String, serde_json::Value>,
        action:   &str,
        resource: &str,
    ) -> Option<Effect>;
}
```

---

### 5.3 `rskit-discovery`

**What gokit has:** `Discovery` interface, `ServiceInstance` (ID, Address, Port, Tags, Metadata), `Watch()` for change notifications, load-balancing strategies (round-robin, random, least-connections).

**Location:** `crates/rskit-discovery/`

**Key dependencies:**
```toml
rskit-errors = { path = "../rskit-errors" }
reqwest      = { version = "0.12", features = ["json", "rustls-tls"], default-features = false }
tokio-stream = { workspace = true }
```

**Public API:**

```rust
// ── Core types ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceInstance {
    pub id:       String,
    pub name:     String,
    pub address:  String,
    pub port:     u16,
    pub healthy:  bool,
    pub tags:     Vec<String>,
    pub metadata: HashMap<String, String>,
}

impl ServiceInstance {
    pub fn address(&self) -> String;   // "host:port"
}

// ── Discovery trait ────────────────────────────────────────────────────────

#[async_trait]
pub trait Discovery: Send + Sync {
    /// Resolve all healthy instances of a service.
    async fn resolve(&self, service: &str) -> AppResult<Vec<ServiceInstance>>;

    /// Watch for instance changes; stream emits on every change.
    async fn watch(
        &self,
        service: &str,
    ) -> AppResult<impl Stream<Item = AppResult<Vec<ServiceInstance>>>>;

    async fn register(&self, instance: &ServiceInstance) -> AppResult<()>;
    async fn deregister(&self, id: &str) -> AppResult<()>;
}

// ── Load-balancing strategies ─────────────────────────────────────────────

pub trait LoadBalancer: Send + Sync {
    fn pick<'a>(&self, instances: &'a [ServiceInstance]) -> Option<&'a ServiceInstance>;
}

pub struct RoundRobin { counter: AtomicUsize }
pub struct Random;
pub struct LeastConnections { in_flight: DashMap<String, AtomicUsize> }

// ── Consul implementation ─────────────────────────────────────────────────

pub struct ConsulDiscovery { ... }

impl ConsulDiscovery {
    pub fn new(base_url: impl Into<String>) -> Self;
}

#[async_trait]
impl Discovery for ConsulDiscovery { ... }

// ── In-memory implementation (for testing) ────────────────────────────────

pub struct InMemoryDiscovery {
    instances: Arc<tokio::sync::RwLock<HashMap<String, Vec<ServiceInstance>>>>,
}

impl InMemoryDiscovery {
    pub fn new() -> Self;
    pub async fn add(&self, service: &str, instance: ServiceInstance);
    pub async fn remove(&self, service: &str, id: &str);
}
```

---

### 5.4 `rskit-testutil`

**What gokit has:** `NewTestDB()`, fixture loading, mock provider helpers.

**Location:** `crates/rskit-testutil/`

**Key dependencies:**
```toml
rskit-errors    = { path = "../rskit-errors" }
rskit-provider  = { path = "../rskit-provider" }
rskit-database  = { path = "../rskit-database", optional = true }
tokio           = { workspace = true }
uuid            = { workspace = true }

[features]
database = ["dep:rskit-database"]
```

**Public API:**

```rust
// ── Mock provider ─────────────────────────────────────────────────────────

/// In-memory RequestResponse for tests.
pub struct MockProvider<I, O> {
    responses: parking_lot::Mutex<VecDeque<AppResult<O>>>,
    calls:     parking_lot::Mutex<Vec<I>>,
}

impl<I: Clone + Send + Sync, O: Clone + Send + Sync> MockProvider<I, O> {
    pub fn new() -> Self;

    /// Queue a successful response.
    pub fn will_return(&self, response: O) -> &Self;

    /// Queue an error response.
    pub fn will_fail(&self, err: AppError) -> &Self;

    /// Returns a snapshot of all recorded calls.
    pub fn calls(&self) -> Vec<I>;

    /// Returns the number of times the mock was called.
    pub fn call_count(&self) -> usize;
}

#[async_trait]
impl<I, O> RequestResponse<I, O> for MockProvider<I, O> { ... }

// ── Test database ─────────────────────────────────────────────────────────

#[cfg(feature = "database")]
/// Creates an isolated SQLite in-memory database for tests.
pub async fn test_database() -> AppResult<Arc<Database>>;

#[cfg(feature = "database")]
/// Run migrations from the given directory against a test DB.
pub async fn run_migrations(db: &Database, migrations_path: &str) -> AppResult<()>;

// ── Assertion helpers ─────────────────────────────────────────────────────

/// Assert that an AppResult is Ok and return its value.
/// Panics with a useful message on Err.
#[track_caller]
pub fn assert_ok<T>(result: AppResult<T>) -> T;

/// Assert that an AppResult is an Err with the given ErrorCode.
#[track_caller]
pub fn assert_err_code(result: AppResult<impl std::fmt::Debug>, code: ErrorCode);
```

---

## 6. Phase 5 — Specialist Crates

### 6.1 `rskit-sse`

**What gokit has:** Server-Sent Events client/server — `Subscribe()`, `Publish()`, `Broadcast()`.

**Location:** `crates/rskit-sse/`

**Key dependencies:**
```toml
axum          = { workspace = true }       # SSE via axum::response::sse
tokio         = { workspace = true }
rskit-errors  = { path = "../rskit-errors" }
```

**Public API:**

```rust
/// In-process SSE bus — multiple subscribers, broadcast or targeted send.
pub struct SseBus<T: Clone + Send + Sync + 'static> {
    tx: tokio::sync::broadcast::Sender<T>,
}

impl<T: Clone + Send + Sync + Serialize + 'static> SseBus<T> {
    pub fn new(capacity: usize) -> Self;

    /// Broadcast an event to all subscribers.
    pub fn publish(&self, event: T) -> AppResult<()>;

    /// Returns a Stream suitable for passing to axum's `Sse` response.
    pub fn subscribe(&self) -> impl Stream<Item = AppResult<axum::response::sse::Event>>;
}

/// Helper: create an axum SSE handler that streams from a `SseBus`.
pub fn sse_handler<T>(bus: Arc<SseBus<T>>) -> impl axum::handler::Handler<...>;
```

---

### 6.2 `rskit-dag`

**What gokit has:** DAG task orchestrator — `AddNode()`, `AddEdge()`, `Execute()`, `TopologicalSort()`, cycle detection, parallel execution where the graph allows.

**Location:** `crates/rskit-dag/`

**Key dependencies:**
```toml
tokio        = { workspace = true }
rskit-errors = { path = "../rskit-errors" }
```

**Public API:**

```rust
/// A node in the DAG.
pub trait DagNode: Send + Sync + 'static {
    fn id(&self) -> &str;
    fn execute(
        &self,
        inputs: HashMap<String, serde_json::Value>,
        cancel: CancellationToken,
    ) -> BoxFuture<'_, AppResult<serde_json::Value>>;
}

pub struct Dag {
    nodes: HashMap<String, Arc<dyn DagNode>>,
    edges: HashMap<String, Vec<String>>,   // node_id → downstream ids
}

impl Dag {
    pub fn new() -> Self;

    #[must_use] pub fn add_node(self, node: impl DagNode) -> Self;

    /// `from` must complete before `to` starts.
    #[must_use] pub fn add_edge(self, from: &str, to: &str) -> AppResult<Self>;

    /// Returns nodes in topological order. Returns Err on cycle.
    pub fn topological_sort(&self) -> AppResult<Vec<String>>;

    /// Execute all nodes. Nodes with no dependencies run in parallel.
    /// Output of each node is passed as `inputs` to downstream nodes.
    pub async fn execute(
        &self,
        cancel: CancellationToken,
    ) -> AppResult<HashMap<String, serde_json::Value>>;
}
```

---

### 6.3 `rskit-llm`

**What gokit has:** LLM provider abstractions — `Complete()`, `Embed()`, `Chat()` with pluggable backends (OpenAI, Claude, etc.).

**Location:** `crates/rskit-llm/`

**Key dependencies:**
```toml
reqwest      = { version = "0.12", features = ["json", "rustls-tls", "stream"], default-features = false }
rskit-errors = { path = "../rskit-errors" }
rskit-resilience = { path = "../rskit-resilience" }
tokio-stream = { workspace = true }
```

**Public API:**

```rust
// ── Core types ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChatMessage {
    pub role:    Role,
    pub content: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Role { System, User, Assistant }

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionRequest {
    pub model:        String,
    pub messages:     Vec<ChatMessage>,
    pub max_tokens:   Option<u32>,
    pub temperature:  Option<f32>,
    pub stream:       bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CompletionResponse {
    pub id:      String,
    pub content: String,
    pub model:   String,
    pub usage:   TokenUsage,
}

#[derive(Debug, Clone, Deserialize)]
pub struct TokenUsage {
    pub input_tokens:  u32,
    pub output_tokens: u32,
}

// ── Provider trait ────────────────────────────────────────────────────────

#[async_trait]
pub trait LlmProvider: Send + Sync {
    async fn complete(&self, req: CompletionRequest)  -> AppResult<CompletionResponse>;
    async fn embed(&self, texts: Vec<String>)          -> AppResult<Vec<Vec<f32>>>;

    /// Returns a stream of delta chunks (for streaming completions).
    async fn complete_stream(
        &self,
        req: CompletionRequest,
    ) -> AppResult<impl Stream<Item = AppResult<String>>>;
}

// ── Implementations ────────────────────────────────────────────────────────

pub struct OpenAiProvider  { ... }
pub struct AnthropicProvider { ... }

pub struct OpenAiConfig {
    pub api_key:     String,
    pub base_url:    String,           // default: https://api.openai.com/v1
    pub timeout:     Duration,
    pub max_retries: u32,
}

pub struct AnthropicConfig {
    pub api_key:   String,
    pub base_url:  String,             // default: https://api.anthropic.com
    pub version:   String,             // default: "2023-06-01"
    pub timeout:   Duration,
    pub max_retries: u32,
}
```

---

## 7. Workspace Changes

### `Cargo.toml` — new members

```toml
members = [
    # existing
    "crates/rskit",
    "crates/rskit-errors",
    "crates/rskit-config",
    "crates/rskit-logging",
    "crates/rskit-bootstrap",
    "crates/rskit-provider",
    "crates/rskit-pipeline",
    "crates/rskit-resilience",
    "crates/rskit-worker",
    "crates/rskit-server",
    # phase 1 enhancements — no new crates
    # phase 2 new crates
    "crates/rskit-validation",
    "crates/rskit-http",
    "crates/rskit-di",
    "crates/rskit-auth",
    # phase 3 adapters
    "crates/rskit-database",
    "crates/rskit-cache",
    "crates/rskit-messaging",
    # phase 4 platform
    "crates/rskit-observability",
    "crates/rskit-authz",
    "crates/rskit-discovery",
    "crates/rskit-testutil",
    # phase 5 specialist
    "crates/rskit-sse",
    "crates/rskit-dag",
    "crates/rskit-llm",
]
```

### New workspace dependencies to add

```toml
# HTTP server (Phase 2)
axum            = { version = "0.8", features = ["http2", "ws"] }
tower-http      = { version = "0.6", features = ["cors", "trace", "request-id", "timeout"] }
hyper           = { version = "1", features = ["full"] }
hyper-util      = { version = "0.1", features = ["tokio"] }

# Auth (Phase 2)
jsonwebtoken    = "9"
argon2          = "0.5"

# Database (Phase 3)
sqlx            = { version = "0.8", features = ["runtime-tokio", "tls-rustls", "postgres", "mysql", "sqlite", "uuid", "chrono"] }

# Cache (Phase 3)
redis           = { version = "0.27", features = ["tokio-comp", "connection-manager"] }

# Messaging (Phase 3)
rdkafka         = { version = "0.37", features = ["tokio"] }

# Observability (Phase 4)
opentelemetry        = "0.27"
opentelemetry_sdk    = { version = "0.27", features = ["rt-tokio"] }
opentelemetry-otlp   = { version = "0.27", features = ["grpc-tonic", "metrics"] }
opentelemetry-prometheus = "0.27"
prometheus           = "0.13"
tracing-opentelemetry = "0.28"

# HTTP client (shared)
reqwest         = { version = "0.12", features = ["json", "rustls-tls"], default-features = false }

# Concurrent hash map (discovery LeastConnections)
dashmap         = "6"
```

### `rskit` facade — new feature flags

```toml
[features]
default  = []
server   = ["dep:rskit-server"]
http     = ["dep:rskit-http"]
auth     = ["dep:rskit-auth"]
database = ["dep:rskit-database"]
cache    = ["dep:rskit-cache"]
messaging = ["dep:rskit-messaging"]
observability = ["dep:rskit-observability"]
authz    = ["dep:rskit-authz"]
discovery = ["dep:rskit-discovery"]
di       = ["dep:rskit-di"]
sse      = ["dep:rskit-sse"]
dag      = ["dep:rskit-dag"]
llm      = ["dep:rskit-llm"]
full     = [
    "server", "http", "auth", "database", "cache", "messaging",
    "observability", "authz", "discovery", "di", "sse", "dag", "llm",
]
```

---

## 8. Dependency Reference

| New dep | Used by | Reason |
|---|---|---|
| `axum 0.8` | rskit-http, rskit-sse | HTTP server, SSE response |
| `tower-http 0.6` | rskit-http | CORS, tracing, request-ID layers |
| `hyper 1` + `hyper-util 0.1` | rskit-http | h2c (HTTP/2 cleartext) |
| `jsonwebtoken 9` | rskit-auth | JWT sign/verify |
| `argon2 0.5` | rskit-auth | Password hashing |
| `sqlx 0.8` | rskit-database | Async DB with compile-time checks |
| `redis 0.27` | rskit-cache | Redis client (connection manager) |
| `rdkafka 0.37` | rskit-messaging | Kafka producer/consumer |
| `opentelemetry* 0.27` | rskit-observability | OTel SDK + OTLP export |
| `opentelemetry-prometheus 0.27` | rskit-observability | Prometheus metrics endpoint |
| `tracing-opentelemetry 0.28` | rskit-observability | Bridge tracing spans → OTel |
| `reqwest 0.12` | rskit-auth, rskit-discovery, rskit-llm | HTTP client (rustls, no OpenSSL) |
| `dashmap 6` | rskit-discovery | Lock-free concurrent HashMap |
| `regex 1` | rskit-validation | Pattern validation |

All dependencies use `rustls` (not `openssl`) to keep the dependency tree portable and avoid system library requirements.

---

## 9. Implementation Order

Work is sequenced so each phase only depends on the previous:

```
Phase 1 — Enhancements to existing crates       (no new crates, low risk)
  ├─ rskit-errors:     add ErrorResponse
  ├─ rskit-logging:    add context helpers + global init
  ├─ rskit-resilience: add callbacks to CB / Retry / Bulkhead
  └─ rskit-bootstrap:  add run_task, concurrent startup, LazyComponent

Phase 2 — New core crates                        (no external infra needed)
  ├─ rskit-validation  (pure logic, no I/O)
  ├─ rskit-di          (pure logic, no I/O)
  ├─ rskit-auth        (JWT + password = no infra; OIDC needs HTTP)
  └─ rskit-http        (depends on axum, tower; integrates with bootstrap)

Phase 3 — Infrastructure adapters               (require running infra for tests)
  ├─ rskit-database    (sqlx; tests need Postgres/MySQL or SQLite)
  ├─ rskit-cache       (redis crate; tests need Redis or use mock)
  └─ rskit-messaging   (rdkafka; tests need Kafka or use in-memory mock)

Phase 4 — Platform crates                        (depends on Phase 2 + 3)
  ├─ rskit-observability  (depends on rskit-logging)
  ├─ rskit-authz          (depends on rskit-auth)
  ├─ rskit-discovery      (depends on rskit-bootstrap)
  └─ rskit-testutil        (depends on rskit-database optional)

Phase 5 — Specialist crates                      (optional, app-domain specific)
  ├─ rskit-sse   (depends on rskit-http/axum)
  ├─ rskit-dag   (self-contained async orchestration)
  └─ rskit-llm   (depends on reqwest)
```

### Estimated crate count

| Phase | Crates | Status |
|---|---|---|
| Existing | 10 | Complete |
| Phase 1 enhancements | 0 new | In spec |
| Phase 2 | +4 | In spec |
| Phase 3 | +3 | In spec |
| Phase 4 | +4 | In spec |
| Phase 5 | +3 | In spec |
| **Total** | **24** | |
