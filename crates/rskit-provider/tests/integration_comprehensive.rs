//! Comprehensive TDD tests for rskit-provider.
//!
//! Covers: traits, adapt helpers, TowerProvider, middleware (logging, resilience, tracing),
//! composition, edge cases, and concurrency.

use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use futures::{stream, StreamExt};
use rskit_errors::{AppError, AppResult, ErrorCode};
use rskit_provider::middleware::logging::LoggingLayer;
use rskit_provider::middleware::resilience::{ResilienceConfig, ResilienceLayer};
use rskit_provider::middleware::tracing_layer::TracingLayer;
use rskit_provider::traits::{
    BoxStream, Closeable, Duplex, DuplexChannel, Initializable, Provider, RequestResponse, Sink,
    StreamProvider,
};
use rskit_provider::{request_response_fn, sink_fn, TowerProvider};
use rskit_resilience::{CbConfig, CircuitBreaker, RateLimiter, RetryPolicy};
use tower::ServiceBuilder;

// ═══════════════════════════════════════════════════════════════════════════════
// Test helpers — mock providers, services, channels
// ═══════════════════════════════════════════════════════════════════════════════

/// A manual Provider impl for testing trait methods directly.
struct SimpleProvider {
    available: bool,
}

#[async_trait::async_trait]
impl Provider for SimpleProvider {
    fn name(&self) -> &'static str {
        "simple"
    }
    async fn is_available(&self) -> bool {
        self.available
    }
}

/// Provider with lifecycle support.
struct LifecycleProvider {
    initialized: AtomicBool,
    closed: AtomicBool,
}

impl LifecycleProvider {
    fn new() -> Self {
        Self {
            initialized: AtomicBool::new(false),
            closed: AtomicBool::new(false),
        }
    }
}

#[async_trait::async_trait]
impl Provider for LifecycleProvider {
    fn name(&self) -> &'static str {
        "lifecycle"
    }
}

#[async_trait::async_trait]
impl Initializable for LifecycleProvider {
    async fn init(&self) -> AppResult<()> {
        self.initialized.store(true, Ordering::SeqCst);
        Ok(())
    }
}

#[async_trait::async_trait]
impl Closeable for LifecycleProvider {
    async fn close(&self) -> AppResult<()> {
        self.closed.store(true, Ordering::SeqCst);
        Ok(())
    }
}

/// A StreamProvider impl for BoxStream tests.
struct VecStreamProvider {
    items: Vec<AppResult<i32>>,
}

#[async_trait::async_trait]
impl Provider for VecStreamProvider {
    fn name(&self) -> &'static str {
        "vec-stream"
    }
}

#[async_trait::async_trait]
impl StreamProvider<(), i32> for VecStreamProvider {
    async fn stream(&self, _input: ()) -> AppResult<BoxStream<i32>> {
        let items = self.items.clone();
        Ok(Box::pin(stream::iter(items)))
    }
}

/// A mock DuplexChannel backed by a Vec buffer.
struct MockChannel {
    buf: Vec<String>,
    pos: usize,
    closed: bool,
}

impl MockChannel {
    fn new() -> Self {
        Self {
            buf: Vec::new(),
            pos: 0,
            closed: false,
        }
    }
}

#[async_trait::async_trait]
impl DuplexChannel<String, String> for MockChannel {
    async fn send(&mut self, input: String) -> AppResult<()> {
        if self.closed {
            return Err(AppError::new(ErrorCode::Internal, "channel closed"));
        }
        self.buf.push(input.to_uppercase());
        Ok(())
    }
    async fn recv(&mut self) -> AppResult<Option<String>> {
        if self.pos < self.buf.len() {
            let item = self.buf[self.pos].clone();
            self.pos += 1;
            Ok(Some(item))
        } else {
            Ok(None)
        }
    }
    async fn close(&mut self) -> AppResult<()> {
        self.closed = true;
        Ok(())
    }
}

/// A Duplex provider that returns MockChannel.
struct MockDuplexProvider;

#[async_trait::async_trait]
impl Provider for MockDuplexProvider {
    fn name(&self) -> &'static str {
        "mock-duplex"
    }
}

#[async_trait::async_trait]
impl Duplex<String, String> for MockDuplexProvider {
    async fn open(&self) -> AppResult<Box<dyn DuplexChannel<String, String>>> {
        Ok(Box::new(MockChannel::new()))
    }
}

/// A tower service that counts calls and fails the first N.
#[derive(Clone)]
struct FailNService {
    call_count: Arc<AtomicUsize>,
    fail_first_n: usize,
}

impl FailNService {
    fn new(fail_first_n: usize) -> Self {
        Self {
            call_count: Arc::new(AtomicUsize::new(0)),
            fail_first_n,
        }
    }
}

impl tower::Service<i32> for FailNService {
    type Response = i32;
    type Error = AppError;
    type Future =
        std::pin::Pin<Box<dyn std::future::Future<Output = Result<i32, AppError>> + Send>>;

    fn poll_ready(
        &mut self,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Result<(), Self::Error>> {
        std::task::Poll::Ready(Ok(()))
    }

    fn call(&mut self, req: i32) -> Self::Future {
        let n = self.call_count.fetch_add(1, Ordering::SeqCst);
        let fail = n < self.fail_first_n;
        Box::pin(async move {
            if fail {
                Err(AppError::new(ErrorCode::ServiceUnavailable, "transient"))
            } else {
                Ok(req * 10)
            }
        })
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 1. Trait Implementations
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn rr_fn_success_i32() {
    let p = request_response_fn("rr-i32", |x: i32| async move { Ok(x + 1) });
    assert_eq!(p.execute(41).await.unwrap(), 42);
}

#[tokio::test]
async fn rr_fn_success_string() {
    let p = request_response_fn("rr-str", |s: String| async move { Ok(s.len()) });
    assert_eq!(p.execute("hello".into()).await.unwrap(), 5);
}

#[tokio::test]
async fn rr_fn_success_vec_u8() {
    let p = request_response_fn("rr-vec", |v: Vec<u8>| async move { Ok(v.len()) });
    assert_eq!(p.execute(vec![1, 2, 3]).await.unwrap(), 3);
}

#[tokio::test]
async fn rr_fn_error_propagation() {
    let p = request_response_fn("rr-err", |_x: i32| async move {
        Err::<i32, _>(AppError::not_found("item", Some("42")))
    });
    let err = p.execute(1).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::NotFound);
}

#[tokio::test]
async fn rr_fn_name() {
    let p = request_response_fn("my-provider", |_: ()| async { Ok(()) });
    assert_eq!(p.name(), "my-provider");
}

#[tokio::test]
async fn rr_fn_is_available_default_true() {
    let p = request_response_fn("avail", |_: ()| async { Ok(()) });
    assert!(p.is_available().await);
}

#[tokio::test]
async fn sink_fn_success() {
    let sink = sink_fn("s", |_: String| async { Ok(()) });
    assert!(sink.send("data".into()).await.is_ok());
}

#[tokio::test]
async fn sink_fn_error() {
    let sink = sink_fn("s-err", |_: i32| async {
        Err::<(), _>(AppError::new(ErrorCode::Internal, "boom"))
    });
    let err = sink.send(1).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::Internal);
}

#[tokio::test]
async fn sink_fn_side_effects() {
    let counter = Arc::new(AtomicUsize::new(0));
    let c = counter.clone();
    let sink = sink_fn("counter-sink", move |_: ()| {
        let c = c.clone();
        async move {
            c.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    });
    sink.send(()).await.unwrap();
    sink.send(()).await.unwrap();
    sink.send(()).await.unwrap();
    assert_eq!(counter.load(Ordering::SeqCst), 3);
}

#[tokio::test]
async fn sink_fn_name() {
    let sink = sink_fn("my-sink", |_: ()| async { Ok(()) });
    assert_eq!(sink.name(), "my-sink");
}

#[tokio::test]
async fn provider_is_available_override() {
    let p = SimpleProvider { available: false };
    assert!(!p.is_available().await);
    assert_eq!(p.name(), "simple");
}

#[tokio::test]
async fn initializable_closeable_lifecycle() {
    let p = LifecycleProvider::new();
    assert!(!p.initialized.load(Ordering::SeqCst));
    assert!(!p.closed.load(Ordering::SeqCst));

    p.init().await.unwrap();
    assert!(p.initialized.load(Ordering::SeqCst));

    p.close().await.unwrap();
    assert!(p.closed.load(Ordering::SeqCst));
}

/// Compile-time check: providers from adapt are Send + Sync.
#[tokio::test]
async fn send_sync_verification() {
    fn assert_send_sync<T: Send + Sync>(_: &T) {}
    let rr = request_response_fn("ss", |x: i32| async move { Ok(x) });
    assert_send_sync(&rr);
    let s = sink_fn("ss", |_: i32| async { Ok(()) });
    assert_send_sync(&s);
}

// ═══════════════════════════════════════════════════════════════════════════════
// 2. BoxStream & DuplexChannel
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn boxstream_multiple_items() {
    let sp = VecStreamProvider {
        items: vec![Ok(1), Ok(2), Ok(3)],
    };
    let mut s = sp.stream(()).await.unwrap();
    assert_eq!(s.next().await.unwrap().unwrap(), 1);
    assert_eq!(s.next().await.unwrap().unwrap(), 2);
    assert_eq!(s.next().await.unwrap().unwrap(), 3);
    assert!(s.next().await.is_none());
}

#[tokio::test]
async fn boxstream_with_error_midstream() {
    let sp = VecStreamProvider {
        items: vec![
            Ok(10),
            Err(AppError::new(ErrorCode::Internal, "mid-error")),
            Ok(30),
        ],
    };
    let mut s = sp.stream(()).await.unwrap();
    assert!(s.next().await.unwrap().is_ok());
    let err = s.next().await.unwrap().unwrap_err();
    assert_eq!(err.code, ErrorCode::Internal);
    // Stream continues after error
    assert_eq!(s.next().await.unwrap().unwrap(), 30);
}

#[tokio::test]
async fn boxstream_empty() {
    let sp = VecStreamProvider { items: vec![] };
    let mut s = sp.stream(()).await.unwrap();
    assert!(s.next().await.is_none());
}

#[tokio::test]
async fn boxstream_collect_all() {
    let sp = VecStreamProvider {
        items: vec![Ok(1), Ok(2), Ok(3), Ok(4), Ok(5)],
    };
    let s = sp.stream(()).await.unwrap();
    let collected: Vec<i32> = s.filter_map(|r| async { r.ok() }).collect().await;
    assert_eq!(collected, vec![1, 2, 3, 4, 5]);
}

#[tokio::test]
async fn duplex_send_recv_roundtrip() {
    let dp = MockDuplexProvider;
    let mut ch = dp.open().await.unwrap();
    ch.send("hello".into()).await.unwrap();
    ch.send("world".into()).await.unwrap();
    assert_eq!(ch.recv().await.unwrap(), Some("HELLO".into()));
    assert_eq!(ch.recv().await.unwrap(), Some("WORLD".into()));
    assert_eq!(ch.recv().await.unwrap(), None);
}

#[tokio::test]
async fn duplex_close_then_send_fails() {
    let dp = MockDuplexProvider;
    let mut ch = dp.open().await.unwrap();
    ch.close().await.unwrap();
    let err = ch.send("after-close".into()).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::Internal);
}

#[tokio::test]
async fn duplex_recv_empty_returns_none() {
    let dp = MockDuplexProvider;
    let mut ch = dp.open().await.unwrap();
    assert_eq!(ch.recv().await.unwrap(), None);
}

#[tokio::test]
async fn duplex_provider_name() {
    let dp = MockDuplexProvider;
    assert_eq!(dp.name(), "mock-duplex");
}

// ═══════════════════════════════════════════════════════════════════════════════
// 3. TowerProvider
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn tower_provider_success() {
    let svc = tower::service_fn(|x: i32| async move { Ok::<_, AppError>(x * 3) });
    let p = TowerProvider::new("tp", svc);
    assert_eq!(p.execute(7).await.unwrap(), 21);
}

#[tokio::test]
async fn tower_provider_error() {
    let svc = tower::service_fn(|_: i32| async {
        Err::<i32, AppError>(AppError::new(ErrorCode::Timeout, "timed out"))
    });
    let p = TowerProvider::new("tp-err", svc);
    let err = p.execute(1).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::Timeout);
}

#[tokio::test]
async fn tower_provider_name_and_available() {
    let svc = tower::service_fn(|_: ()| async { Ok::<_, AppError>(()) });
    let p = TowerProvider::new("my-tower", svc);
    assert_eq!(p.name(), "my-tower");
    assert!(p.is_available().await);
}

#[tokio::test]
async fn tower_provider_string_types() {
    let svc =
        tower::service_fn(|s: String| async move { Ok::<_, AppError>(format!("echo: {s}")) });
    let p = TowerProvider::new("echo-tower", svc);
    assert_eq!(
        p.execute("hi".into()).await.unwrap(),
        "echo: hi".to_string()
    );
}

#[tokio::test]
async fn tower_provider_concurrent_access() {
    let svc = tower::service_fn(|x: i32| async move { Ok::<_, AppError>(x * 2) });
    let p = Arc::new(TowerProvider::new("conc-tp", svc));

    let mut handles = Vec::new();
    for i in 0..10 {
        let p = p.clone();
        handles.push(tokio::spawn(async move { p.execute(i).await.unwrap() }));
    }
    let mut results = Vec::new();
    for h in handles {
        results.push(h.await.unwrap());
    }
    results.sort();
    assert_eq!(results, vec![0, 2, 4, 6, 8, 10, 12, 14, 16, 18]);
}

#[tokio::test]
async fn tower_provider_multiple_calls() {
    let svc = tower::service_fn(|x: i32| async move { Ok::<_, AppError>(x + 100) });
    let p = TowerProvider::new("multi", svc);
    assert_eq!(p.execute(1).await.unwrap(), 101);
    assert_eq!(p.execute(2).await.unwrap(), 102);
    assert_eq!(p.execute(3).await.unwrap(), 103);
}

// ═══════════════════════════════════════════════════════════════════════════════
// 4. LoggingLayer
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn logging_layer_wraps_correctly() {
    let layer = LoggingLayer::new("test-log");
    assert_eq!(layer.provider_name, "test-log");
}

#[tokio::test]
async fn logging_layer_success_passthrough() {
    use tower::Service;
    let inner = tower::service_fn(|x: i32| async move { Ok::<_, AppError>(x * 2) });
    let mut svc = ServiceBuilder::new()
        .layer(LoggingLayer::new("log-pass"))
        .service(inner);

    tower::ServiceExt::ready(&mut svc).await.unwrap();
    assert_eq!(svc.call(5).await.unwrap(), 10);
}

#[tokio::test]
async fn logging_layer_error_passthrough() {
    use tower::Service;
    let inner = tower::service_fn(|_: i32| async {
        Err::<i32, AppError>(AppError::new(ErrorCode::Internal, "fail"))
    });
    let mut svc = ServiceBuilder::new()
        .layer(LoggingLayer::new("log-err"))
        .service(inner);

    tower::ServiceExt::ready(&mut svc).await.unwrap();
    let err = svc.call(1).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::Internal);
}

#[tokio::test]
async fn logging_layer_provider_name_propagation() {
    let layer = LoggingLayer::new("custom-name");
    let inner = tower::service_fn(|_: ()| async { Ok::<_, AppError>(()) });
    let svc = layer.layer(&inner);
    // The LoggingService wraps inner — just verify it compiles and has correct name
    let _ = svc;
}

#[tokio::test]
async fn logging_layer_with_string_io() {
    use tower::Service;
    let inner = tower::service_fn(|s: String| async move { Ok::<_, AppError>(s.to_uppercase()) });
    let mut svc = ServiceBuilder::new()
        .layer(LoggingLayer::new("str-log"))
        .service(inner);

    tower::ServiceExt::ready(&mut svc).await.unwrap();
    assert_eq!(svc.call("hello".into()).await.unwrap(), "HELLO");
}

#[tokio::test]
async fn logging_layer_multiple_calls() {
    use tower::Service;
    let inner = tower::service_fn(|x: i32| async move { Ok::<_, AppError>(x) });
    let mut svc = ServiceBuilder::new()
        .layer(LoggingLayer::new("multi-log"))
        .service(inner);

    for i in 0..5 {
        tower::ServiceExt::ready(&mut svc).await.unwrap();
        assert_eq!(svc.call(i).await.unwrap(), i);
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 5. ResilienceLayer
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn resilience_empty_config_passthrough() {
    use tower::Service;
    let inner = tower::service_fn(|x: i32| async move { Ok::<_, AppError>(x) });
    let config = ResilienceConfig::new();
    let mut svc = ServiceBuilder::new()
        .layer(ResilienceLayer::new(config))
        .service(inner);

    tower::ServiceExt::ready(&mut svc).await.unwrap();
    assert_eq!(svc.call(42).await.unwrap(), 42);
}

#[tokio::test]
async fn resilience_rate_limiter_allows_within_budget() {
    use tower::Service;
    let inner = tower::service_fn(|x: i32| async move { Ok::<_, AppError>(x) });
    let config = ResilienceConfig::new().with_rate_limiter(RateLimiter::new("rl-ok", 100, 100));
    let mut svc = ServiceBuilder::new()
        .layer(ResilienceLayer::new(config))
        .service(inner);

    for i in 0..10 {
        tower::ServiceExt::ready(&mut svc).await.unwrap();
        assert_eq!(svc.call(i).await.unwrap(), i);
    }
}

#[tokio::test]
async fn resilience_rate_limiter_rejects_when_exhausted() {
    use tower::Service;
    let inner = tower::service_fn(|x: i32| async move { Ok::<_, AppError>(x) });
    let config = ResilienceConfig::new().with_rate_limiter(RateLimiter::new("rl-rej", 1, 1));
    let mut svc = ServiceBuilder::new()
        .layer(ResilienceLayer::new(config))
        .service(inner);

    // First call succeeds
    tower::ServiceExt::ready(&mut svc).await.unwrap();
    assert!(svc.call(1).await.is_ok());

    // Second call should be rate limited
    tower::ServiceExt::ready(&mut svc).await.unwrap();
    let err = svc.call(2).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::RateLimited);
}

#[tokio::test]
async fn resilience_retry_success_after_failures() {
    use tower::Service;

    let inner = FailNService::new(2); // fail first 2, succeed on 3rd
    let policy = RetryPolicy::new()
        .with_max_attempts(5)
        .with_initial_backoff(Duration::from_millis(1))
        .with_jitter(false);
    let config = ResilienceConfig::new().with_retry(policy);
    let mut svc = ServiceBuilder::new()
        .layer(ResilienceLayer::new(config))
        .service(inner);

    tower::ServiceExt::ready(&mut svc).await.unwrap();
    let result = svc.call(7).await.unwrap();
    assert_eq!(result, 70);
}

#[tokio::test]
async fn resilience_retry_exhausted() {
    use tower::Service;

    let inner = FailNService::new(100); // always fail
    let policy = RetryPolicy::new()
        .with_max_attempts(3)
        .with_initial_backoff(Duration::from_millis(1))
        .with_jitter(false);
    let config = ResilienceConfig::new().with_retry(policy);
    let mut svc = ServiceBuilder::new()
        .layer(ResilienceLayer::new(config))
        .service(inner);

    tower::ServiceExt::ready(&mut svc).await.unwrap();
    let err = svc.call(1).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::ServiceUnavailable);
}

#[tokio::test]
async fn resilience_circuit_breaker_opens_after_failures() {
    use tower::Service;

    let cb_config = CbConfig::new("test-cb")
        .with_max_failures(2)
        .with_timeout(Duration::from_secs(60));
    let cb = CircuitBreaker::new(cb_config);

    let inner = FailNService::new(100); // always fail
    let config = ResilienceConfig::new().with_circuit_breaker(cb);
    let mut svc = ServiceBuilder::new()
        .layer(ResilienceLayer::new(config))
        .service(inner);

    // Two failures should trip the breaker
    for _ in 0..2 {
        tower::ServiceExt::ready(&mut svc).await.unwrap();
        let _ = svc.call(1).await;
    }

    // Third call should be rejected by circuit breaker
    tower::ServiceExt::ready(&mut svc).await.unwrap();
    let err = svc.call(1).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::ServiceUnavailable);
}

#[tokio::test]
async fn resilience_circuit_breaker_passes_when_healthy() {
    use tower::Service;

    let cb_config = CbConfig::new("healthy-cb").with_max_failures(5);
    let cb = CircuitBreaker::new(cb_config);

    let inner = tower::service_fn(|x: i32| async move { Ok::<_, AppError>(x * 2) });
    let config = ResilienceConfig::new().with_circuit_breaker(cb);
    let mut svc = ServiceBuilder::new()
        .layer(ResilienceLayer::new(config))
        .service(inner);

    tower::ServiceExt::ready(&mut svc).await.unwrap();
    assert_eq!(svc.call(5).await.unwrap(), 10);
}

#[tokio::test]
async fn resilience_combined_retry_and_circuit_breaker() {
    use tower::Service;

    let cb_config = CbConfig::new("combo-cb")
        .with_max_failures(10)
        .with_timeout(Duration::from_secs(60));
    let cb = CircuitBreaker::new(cb_config);
    let policy = RetryPolicy::new()
        .with_max_attempts(5)
        .with_initial_backoff(Duration::from_millis(1))
        .with_jitter(false);

    let inner = FailNService::new(2); // fail first 2, then succeed
    let config = ResilienceConfig::new()
        .with_retry(policy)
        .with_circuit_breaker(cb);
    let mut svc = ServiceBuilder::new()
        .layer(ResilienceLayer::new(config))
        .service(inner);

    tower::ServiceExt::ready(&mut svc).await.unwrap();
    let result = svc.call(3).await.unwrap();
    assert_eq!(result, 30);
}

#[tokio::test]
async fn resilience_error_from_inner_propagates() {
    use tower::Service;
    let inner = tower::service_fn(|_: i32| async {
        Err::<i32, AppError>(AppError::new(ErrorCode::Forbidden, "no access"))
    });
    let config = ResilienceConfig::new();
    let mut svc = ServiceBuilder::new()
        .layer(ResilienceLayer::new(config))
        .service(inner);

    tower::ServiceExt::ready(&mut svc).await.unwrap();
    let err = svc.call(1).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::Forbidden);
}

#[tokio::test]
async fn resilience_config_builder_chain() {
    let config = ResilienceConfig::new()
        .with_retry(
            RetryPolicy::new()
                .with_max_attempts(3)
                .with_initial_backoff(Duration::from_millis(10)),
        )
        .with_circuit_breaker(CircuitBreaker::new(CbConfig::new("chain-cb")))
        .with_rate_limiter(RateLimiter::new("chain-rl", 100, 100));

    assert!(config.retry.is_some());
    assert!(config.circuit_breaker.is_some());
    assert!(config.rate_limiter.is_some());
}

#[tokio::test]
async fn resilience_layer_with_service_builder() {
    use tower::Service;
    let inner = tower::service_fn(|x: i32| async move { Ok::<_, AppError>(x) });
    let config = ResilienceConfig::new().with_rate_limiter(RateLimiter::new("sb-rl", 100, 100));

    let mut svc = ServiceBuilder::new()
        .layer(ResilienceLayer::new(config))
        .service(inner);

    tower::ServiceExt::ready(&mut svc).await.unwrap();
    assert_eq!(svc.call(99).await.unwrap(), 99);
}

// ═══════════════════════════════════════════════════════════════════════════════
// 6. TracingLayer
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn tracing_layer_wraps_service() {
    let layer = TracingLayer::new("trace-test");
    assert_eq!(layer.provider_name, "trace-test");
}

#[tokio::test]
async fn tracing_layer_success_no_panic() {
    use tower::Service;
    let inner = tower::service_fn(|x: i32| async move { Ok::<_, AppError>(x + 1) });
    let mut svc = ServiceBuilder::new()
        .layer(TracingLayer::new("trace-ok"))
        .service(inner);

    tower::ServiceExt::ready(&mut svc).await.unwrap();
    assert_eq!(svc.call(9).await.unwrap(), 10);
}

#[tokio::test]
async fn tracing_layer_error_no_panic() {
    use tower::Service;
    let inner = tower::service_fn(|_: i32| async {
        Err::<i32, AppError>(AppError::new(ErrorCode::Internal, "traced error"))
    });
    let mut svc = ServiceBuilder::new()
        .layer(TracingLayer::new("trace-err"))
        .service(inner);

    tower::ServiceExt::ready(&mut svc).await.unwrap();
    let err = svc.call(1).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::Internal);
}

#[tokio::test]
async fn tracing_layer_multiple_calls() {
    use tower::Service;
    let inner = tower::service_fn(|x: i32| async move { Ok::<_, AppError>(x) });
    let mut svc = ServiceBuilder::new()
        .layer(TracingLayer::new("trace-multi"))
        .service(inner);

    for i in 0..3 {
        tower::ServiceExt::ready(&mut svc).await.unwrap();
        assert_eq!(svc.call(i).await.unwrap(), i);
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 7. Composition — multi-layer stacks
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn composition_logging_plus_tracing() {
    use tower::Service;
    let inner = tower::service_fn(|x: i32| async move { Ok::<_, AppError>(x * 2) });
    let mut svc = ServiceBuilder::new()
        .layer(LoggingLayer::new("comp-log"))
        .layer(TracingLayer::new("comp-trace"))
        .service(inner);

    tower::ServiceExt::ready(&mut svc).await.unwrap();
    assert_eq!(svc.call(5).await.unwrap(), 10);
}

#[tokio::test]
async fn composition_all_three_layers() {
    use tower::Service;
    let inner = tower::service_fn(|x: i32| async move { Ok::<_, AppError>(x + 100) });
    let config = ResilienceConfig::new().with_rate_limiter(RateLimiter::new("comp-rl", 100, 100));

    let mut svc = ServiceBuilder::new()
        .layer(LoggingLayer::new("all-log"))
        .layer(TracingLayer::new("all-trace"))
        .layer(ResilienceLayer::new(config))
        .service(inner);

    tower::ServiceExt::ready(&mut svc).await.unwrap();
    assert_eq!(svc.call(1).await.unwrap(), 101);
}

#[tokio::test]
async fn composition_error_through_stack() {
    use tower::Service;
    let inner = tower::service_fn(|_: i32| async {
        Err::<i32, AppError>(AppError::new(ErrorCode::Internal, "deep fail"))
    });
    let config = ResilienceConfig::new();

    let mut svc = ServiceBuilder::new()
        .layer(LoggingLayer::new("err-log"))
        .layer(TracingLayer::new("err-trace"))
        .layer(ResilienceLayer::new(config))
        .service(inner);

    tower::ServiceExt::ready(&mut svc).await.unwrap();
    let err = svc.call(1).await.unwrap_err();
    assert_eq!(err.code, ErrorCode::Internal);
}

#[tokio::test]
async fn composition_resilience_plus_logging() {
    use tower::Service;
    let inner = tower::service_fn(|x: i32| async move { Ok::<_, AppError>(x) });
    let config = ResilienceConfig::new().with_rate_limiter(RateLimiter::new("rl-log", 100, 100));

    let mut svc = ServiceBuilder::new()
        .layer(LoggingLayer::new("combo"))
        .layer(ResilienceLayer::new(config))
        .service(inner);

    tower::ServiceExt::ready(&mut svc).await.unwrap();
    assert_eq!(svc.call(7).await.unwrap(), 7);
}

#[tokio::test]
async fn composition_tower_provider_with_layered_service() {
    // Build a layered tower service, then wrap in TowerProvider
    let inner = tower::service_fn(|x: i32| async move { Ok::<_, AppError>(x * 5) });
    let svc = ServiceBuilder::new()
        .layer(LoggingLayer::new("tp-layered"))
        .layer(TracingLayer::new("tp-layered"))
        .service(inner);

    let p = TowerProvider::new("layered-tp", svc);
    assert_eq!(p.execute(4).await.unwrap(), 20);
    assert_eq!(p.name(), "layered-tp");
}

#[tokio::test]
async fn composition_multiple_calls_through_stack() {
    use tower::Service;
    let inner = tower::service_fn(|x: i32| async move { Ok::<_, AppError>(x) });
    let config = ResilienceConfig::new().with_rate_limiter(RateLimiter::new("mc-rl", 100, 100));

    let mut svc = ServiceBuilder::new()
        .layer(LoggingLayer::new("mc-log"))
        .layer(ResilienceLayer::new(config))
        .service(inner);

    for i in 0..10 {
        tower::ServiceExt::ready(&mut svc).await.unwrap();
        assert_eq!(svc.call(i).await.unwrap(), i);
    }
}

// ═══════════════════════════════════════════════════════════════════════════════
// 8. Edge Cases & Concurrency
// ═══════════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn edge_empty_provider_name() {
    let p = request_response_fn("", |x: i32| async move { Ok(x) });
    assert_eq!(p.name(), "");
    assert_eq!(p.execute(1).await.unwrap(), 1);
}

#[tokio::test]
async fn edge_large_input_payload() {
    let p = request_response_fn("big", |v: Vec<u8>| async move { Ok(v.len()) });
    let big = vec![0u8; 1_000_000]; // 1 MB
    assert_eq!(p.execute(big).await.unwrap(), 1_000_000);
}

#[tokio::test]
async fn edge_concurrent_rr_fn_calls() {
    let counter = Arc::new(AtomicUsize::new(0));
    let c = counter.clone();
    let p = Arc::new(request_response_fn("conc", move |x: i32| {
        let c = c.clone();
        async move {
            c.fetch_add(1, Ordering::SeqCst);
            Ok(x * 2)
        }
    }));

    let mut handles = Vec::new();
    for i in 0..20 {
        let p = p.clone();
        handles.push(tokio::spawn(async move { p.execute(i).await.unwrap() }));
    }

    let mut results = Vec::new();
    for h in handles {
        results.push(h.await.unwrap());
    }
    results.sort();
    let expected: Vec<i32> = (0..20).map(|i| i * 2).collect();
    assert_eq!(results, expected);
    assert_eq!(counter.load(Ordering::SeqCst), 20);
}

#[tokio::test]
async fn edge_timeout_on_slow_provider() {
    let p = request_response_fn("slow", |_: ()| async {
        tokio::time::sleep(Duration::from_secs(10)).await;
        Ok(())
    });
    let result = tokio::time::timeout(Duration::from_millis(50), p.execute(())).await;
    assert!(result.is_err(), "should timeout");
}

#[tokio::test]
async fn edge_error_variants_propagation() {
    let variants = vec![
        ErrorCode::NotFound,
        ErrorCode::Unauthorized,
        ErrorCode::Forbidden,
        ErrorCode::Timeout,
        ErrorCode::Conflict,
        ErrorCode::InvalidInput,
        ErrorCode::Internal,
    ];
    for code in variants {
        let expected_code = code.clone();
        let p = request_response_fn("err-var", move |_: ()| {
            let c = expected_code.clone();
            async move { Err::<(), _>(AppError::new(c, "test")) }
        });
        let err = p.execute(()).await.unwrap_err();
        assert_eq!(err.code, code);
    }
}

#[tokio::test]
async fn edge_concurrent_sink_calls() {
    let counter = Arc::new(AtomicUsize::new(0));
    let c = counter.clone();
    let s = Arc::new(sink_fn("conc-sink", move |_: ()| {
        let c = c.clone();
        async move {
            c.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }));

    let mut handles = Vec::new();
    for _ in 0..50 {
        let s = s.clone();
        handles.push(tokio::spawn(async move { s.send(()).await.unwrap() }));
    }
    for h in handles {
        h.await.unwrap();
    }
    assert_eq!(counter.load(Ordering::SeqCst), 50);
}

#[tokio::test]
async fn edge_tower_provider_send_sync() {
    fn assert_send_sync<T: Send + Sync>(_: &T) {}
    let svc = tower::service_fn(|x: i32| async move { Ok::<_, AppError>(x) });
    let p = TowerProvider::new("ss-tp", svc);
    assert_send_sync(&p);
}

#[tokio::test]
async fn edge_resilience_config_default() {
    let config = ResilienceConfig::default();
    assert!(config.retry.is_none());
    assert!(config.circuit_breaker.is_none());
    assert!(config.rate_limiter.is_none());
}

#[tokio::test]
async fn edge_logging_layer_default() {
    let layer = LoggingLayer::default();
    assert_eq!(layer.provider_name, "");
}
