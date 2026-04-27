//! HTTP rate-limiting Tower layer with per-key token buckets and standard
//! `X-RateLimit-*` response headers.

use std::{
    collections::HashMap,
    convert::Infallible,
    future::Future,
    pin::Pin,
    sync::Arc,
    task::{Context, Poll},
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use axum::{
    body::Body,
    http::{Request, StatusCode},
    response::{IntoResponse, Response},
};
use parking_lot::Mutex;
use tokio_util::sync::CancellationToken;
use tower::{Layer, Service};

// ── Type aliases for complex function pointers ────────────────────────────────

/// Extracts a bucket key from an incoming request (e.g. IP, user-id).
type KeyExtractor = Arc<dyn Fn(&Request<Body>) -> String + Send + Sync>;
/// Returns `(key, rpm)` — allows tiered limits per request.
type LimitExtractor = Arc<dyn Fn(&Request<Body>) -> (String, u32) + Send + Sync>;

// ── Configuration ─────────────────────────────────────────────────────────────

/// Configuration for the HTTP rate limiter.
pub struct RateLimitConfig {
    /// Default requests-per-minute for every bucket.
    pub requests_per_minute: u32,
    /// Extracts a key from the request (e.g. IP, user-id).
    pub key_func: Option<KeyExtractor>,
    /// Returns `(key, rpm)` — allows tiered limits per request.
    pub limit_func: Option<LimitExtractor>,
    /// How often the background task evicts stale buckets.
    pub cleanup_interval: Duration,
    /// Buckets not accessed within this TTL are removed.
    pub bucket_ttl: Duration,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            requests_per_minute: 60,
            key_func: None,
            limit_func: None,
            cleanup_interval: Duration::from_secs(5 * 60),
            bucket_ttl: Duration::from_secs(10 * 60),
        }
    }
}

impl RateLimitConfig {
    /// Set the default requests-per-minute.
    pub fn with_requests_per_minute(mut self, rpm: u32) -> Self {
        self.requests_per_minute = rpm;
        self
    }

    /// Set a key-extraction function.
    pub fn with_key_func(
        mut self,
        f: impl Fn(&Request<Body>) -> String + Send + Sync + 'static,
    ) -> Self {
        self.key_func = Some(Arc::new(f));
        self
    }

    /// Set a limit function that returns `(key, rpm)`.
    pub fn with_limit_func(
        mut self,
        f: impl Fn(&Request<Body>) -> (String, u32) + Send + Sync + 'static,
    ) -> Self {
        self.limit_func = Some(Arc::new(f));
        self
    }

    /// Set the cleanup interval.
    pub fn with_cleanup_interval(mut self, d: Duration) -> Self {
        self.cleanup_interval = d;
        self
    }

    /// Set the bucket TTL.
    pub fn with_bucket_ttl(mut self, d: Duration) -> Self {
        self.bucket_ttl = d;
        self
    }
}

// ── Built-in key extractors ───────────────────────────────────────────────────

/// Extract a rate-limit key from `X-Forwarded-For`, falling back to `"unknown"`.
pub fn ip_based_key(req: &Request<Body>) -> String {
    if let Some(v) = req.headers().get("x-forwarded-for")
        && let Ok(s) = v.to_str()
        && let Some(first) = s.split(',').next()
    {
        let trimmed = first.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }
    "unknown".to_string()
}

/// Extract a rate-limit key from the `user_id` request extension, falling back
/// to [`ip_based_key`].
pub fn user_based_key(req: &Request<Body>) -> String {
    if let Some(uid) = req.extensions().get::<String>() {
        return uid.clone();
    }
    ip_based_key(req)
}

// ── Token bucket ──────────────────────────────────────────────────────────────

struct TokenBucket {
    tokens: f64,
    max_tokens: f64,
    refill_rate: f64, // tokens per second
    last_refill: Instant,
    last_access: Instant,
}

impl TokenBucket {
    fn new(rpm: u32, now: Instant) -> Self {
        let max = rpm as f64;
        Self {
            tokens: max,
            max_tokens: max,
            refill_rate: max / 60.0,
            last_refill: now,
            last_access: now,
        }
    }

    /// Try to consume one token. Returns `(allowed, remaining, retry_after_secs)`.
    fn allow(&mut self, now: Instant) -> (bool, u32, f64) {
        // refill
        let elapsed = now.duration_since(self.last_refill).as_secs_f64();
        self.tokens = (self.tokens + elapsed * self.refill_rate).min(self.max_tokens);
        self.last_refill = now;
        self.last_access = now;

        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            (true, self.tokens as u32, 0.0)
        } else {
            let deficit = 1.0 - self.tokens;
            let retry_after = deficit / self.refill_rate;
            (false, 0, retry_after)
        }
    }
}

// ── HttpRateLimiter (shared state) ────────────────────────────────────────────

/// Shared, multi-key HTTP rate limiter backed by per-key token buckets.
pub struct HttpRateLimiter {
    cfg: RateLimitConfig,
    buckets: Mutex<HashMap<String, TokenBucket>>,
    stop: CancellationToken,
}

impl HttpRateLimiter {
    /// Create a new limiter wrapped in an `Arc` and spawn a background cleanup
    /// task on the current Tokio runtime.
    pub fn new(cfg: RateLimitConfig) -> Arc<Self> {
        let limiter = Arc::new(Self {
            cfg,
            buckets: Mutex::new(HashMap::new()),
            stop: CancellationToken::new(),
        });
        let l = Arc::clone(&limiter);
        tokio::spawn(async move { l.cleanup().await });
        limiter
    }

    /// Check whether a request identified by `key` with the given `rpm` limit
    /// is allowed.
    ///
    /// Returns `(allowed, limit, remaining, retry_after_secs, reset_unix)`.
    pub fn allow(&self, key: &str, rpm: u32) -> (bool, u32, u32, f64, i64) {
        let now_inst = Instant::now();
        let mut map = self.buckets.lock();
        let bucket = map
            .entry(key.to_string())
            .or_insert_with(|| TokenBucket::new(rpm, now_inst));

        // If the limit changed, recreate the bucket so the new RPM applies.
        if (bucket.max_tokens - rpm as f64).abs() > f64::EPSILON {
            *bucket = TokenBucket::new(rpm, now_inst);
        }

        let (allowed, remaining, retry_after) = bucket.allow(now_inst);

        let reset_secs = if allowed {
            // Time until full refill of consumed tokens.
            ((bucket.max_tokens - bucket.tokens) / bucket.refill_rate).ceil() as i64
        } else {
            retry_after.ceil() as i64
        };
        let reset_unix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs() as i64
            + reset_secs;

        (allowed, rpm, remaining, retry_after, reset_unix)
    }

    /// Signal the background cleanup task to stop.
    pub fn stop(&self) {
        self.stop.cancel();
    }

    async fn cleanup(self: Arc<Self>) {
        loop {
            tokio::select! {
                _ = self.stop.cancelled() => break,
                _ = tokio::time::sleep(self.cfg.cleanup_interval) => {
                    let now = Instant::now();
                    let ttl = self.cfg.bucket_ttl;
                    self.buckets.lock().retain(|_, b| {
                        now.duration_since(b.last_access) < ttl
                    });
                }
            }
        }
    }
}

impl Drop for HttpRateLimiter {
    fn drop(&mut self) {
        self.stop.cancel();
    }
}

// ── Tower Layer ───────────────────────────────────────────────────────────────

/// Tower layer that applies per-key HTTP rate limiting.
#[derive(Clone)]
pub struct HttpRateLimitLayer {
    limiter: Arc<HttpRateLimiter>,
}

impl HttpRateLimitLayer {
    /// Wrap an existing [`HttpRateLimiter`].
    pub fn new(limiter: Arc<HttpRateLimiter>) -> Self {
        Self { limiter }
    }
}

impl<S> Layer<S> for HttpRateLimitLayer {
    type Service = HttpRateLimitService<S>;
    fn layer(&self, inner: S) -> Self::Service {
        HttpRateLimitService {
            inner,
            limiter: Arc::clone(&self.limiter),
        }
    }
}

// ── Tower Service ─────────────────────────────────────────────────────────────

/// Service produced by [`HttpRateLimitLayer`].
#[derive(Clone)]
pub struct HttpRateLimitService<S> {
    inner: S,
    limiter: Arc<HttpRateLimiter>,
}

impl<S, ReqBody> Service<Request<ReqBody>> for HttpRateLimitService<S>
where
    S: Service<Request<ReqBody>, Response = Response, Error = Infallible> + Clone + Send + 'static,
    S::Future: Send + 'static,
    ReqBody: Send + 'static,
{
    type Response = Response;
    type Error = Infallible;
    type Future = Pin<Box<dyn Future<Output = Result<Response, Infallible>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request<ReqBody>) -> Self::Future {
        let limiter = Arc::clone(&self.limiter);
        let mut inner = self.inner.clone();
        // Swap so the *ready* clone is used for this request.
        std::mem::swap(&mut self.inner, &mut inner);

        Box::pin(async move {
            // We need to extract key+rpm *before* passing the request on.
            // Because the key/limit funcs expect `Request<Body>`, we must
            // convert. When the body is already `Body` this is zero-cost;
            // otherwise we resolve with defaults.
            let (key, rpm) = resolve_key_and_rpm(&req, &limiter.cfg);

            let (allowed, limit, remaining, retry_after, reset_unix) = limiter.allow(&key, rpm);

            if !allowed {
                let body = serde_json::json!({"error": "rate limit exceeded"}).to_string();
                let resp = (
                    StatusCode::TOO_MANY_REQUESTS,
                    [
                        ("x-ratelimit-limit", limit.to_string()),
                        ("x-ratelimit-remaining", remaining.to_string()),
                        ("x-ratelimit-reset", reset_unix.to_string()),
                        ("retry-after", (retry_after.ceil() as u64).to_string()),
                        ("content-type", "application/json".to_string()),
                    ],
                    body,
                )
                    .into_response();
                return Ok(resp);
            }

            let resp = inner.call(req).await?;

            // Attach rate-limit headers to the successful response.
            let (mut parts, body) = resp.into_parts();
            // SAFETY: u64::to_string() produces ASCII digits, which are always
            // valid HeaderValue bytes; parse() cannot fail for this input.
            parts.headers.insert(
                "x-ratelimit-limit",
                limit
                    .to_string()
                    .parse()
                    .expect("u64 string is valid header value"),
            );
            parts.headers.insert(
                "x-ratelimit-remaining",
                remaining
                    .to_string()
                    .parse()
                    .expect("u64 string is valid header value"),
            );
            parts.headers.insert(
                "x-ratelimit-reset",
                reset_unix
                    .to_string()
                    .parse()
                    .expect("u64 string is valid header value"),
            );
            Ok(Response::from_parts(parts, body))
        })
    }
}

// ── Helpers ───────────────────────────────────────────────────────────────────

/// Resolve the rate-limit key and RPM for a request. When the body type is not
/// `Body` (i.e. the key/limit funcs cannot be called directly) we fall back to
/// the default RPM and `"unknown"` key.
fn resolve_key_and_rpm<B: 'static>(req: &Request<B>, cfg: &RateLimitConfig) -> (String, u32) {
    // Try to downcast the request reference to Request<Body>.
    // Because we are generic over B, we use `std::any::TypeId` to check.
    let is_body = std::any::TypeId::of::<B>() == std::any::TypeId::of::<Body>();

    if is_body {
        // SAFETY: We verify TypeId equality above before casting.
        // This is sound because Request<B> and Request<Body> have identical
        // layout when B == Body (same generic parameter).
        // TODO(#37): Refactor this function to accept &Request<axum::body::Body>
        // directly and remove the unsafe cast entirely.
        let req_ref: &Request<Body> =
            unsafe { &*(req as *const Request<B> as *const Request<Body>) };

        if let Some(ref lf) = cfg.limit_func {
            return lf(req_ref);
        }
        if let Some(ref kf) = cfg.key_func {
            return (kf(req_ref), cfg.requests_per_minute);
        }
    }

    ("unknown".to_string(), cfg.requests_per_minute)
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Instant;

    // ── TokenBucket ───────────────────────────────────────────────────────

    #[test]
    fn bucket_allows_within_limit() {
        let now = Instant::now();
        let mut b = TokenBucket::new(10, now);
        for _ in 0..10 {
            let (ok, _, _) = b.allow(now);
            assert!(ok);
        }
    }

    #[test]
    fn bucket_rejects_when_exhausted() {
        let now = Instant::now();
        let mut b = TokenBucket::new(5, now);
        for _ in 0..5 {
            b.allow(now);
        }
        let (ok, remaining, retry_after) = b.allow(now);
        assert!(!ok);
        assert_eq!(remaining, 0);
        assert!(retry_after > 0.0);
    }

    #[test]
    fn bucket_refills_over_time() {
        let now = Instant::now();
        let mut b = TokenBucket::new(60, now); // 1 token/sec
        // exhaust all tokens
        for _ in 0..60 {
            b.allow(now);
        }
        let (ok, _, _) = b.allow(now);
        assert!(!ok);
        // simulate 2 seconds later → ≥2 tokens refilled
        let later = now + Duration::from_secs(2);
        let (ok, remaining, _) = b.allow(later);
        assert!(ok);
        assert!(remaining <= 60);
    }

    // ── HttpRateLimiter ───────────────────────────────────────────────────

    #[tokio::test]
    async fn limiter_per_key_isolation() {
        let limiter = HttpRateLimiter::new(RateLimitConfig::default().with_requests_per_minute(2));
        // key "a" — use 2 tokens
        assert!(limiter.allow("a", 2).0);
        assert!(limiter.allow("a", 2).0);
        assert!(!limiter.allow("a", 2).0);
        // key "b" should still have tokens
        assert!(limiter.allow("b", 2).0);
        limiter.stop();
    }

    // ── Layer / Service integration ───────────────────────────────────────

    #[tokio::test]
    async fn service_returns_429_when_limited() {
        use tower::ServiceExt;

        let limiter = HttpRateLimiter::new(
            RateLimitConfig::default()
                .with_requests_per_minute(1)
                .with_key_func(|_req: &Request<Body>| "test".to_string()),
        );
        let layer = HttpRateLimitLayer::new(Arc::clone(&limiter));

        // A trivial inner service that returns 200.
        let svc = tower::service_fn(|_req: Request<Body>| async {
            Ok::<_, Infallible>(StatusCode::OK.into_response())
        });
        let mut svc = layer.layer(svc);

        // First request should succeed.
        let req = Request::builder().body(Body::empty()).unwrap();
        let resp = svc.ready().await.unwrap().call(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert!(resp.headers().contains_key("x-ratelimit-limit"));

        // Second request should be rate-limited.
        let req = Request::builder().body(Body::empty()).unwrap();
        let resp = svc.ready().await.unwrap().call(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::TOO_MANY_REQUESTS);
        assert!(resp.headers().contains_key("retry-after"));

        limiter.stop();
    }
}
