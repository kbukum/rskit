use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use rskit_errors::{AppError, AppResult};

/// Observable circuit breaker state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CbState {
    /// Normal operation — requests pass through.
    Closed,
    /// Tripped — requests fail immediately without calling the upstream.
    Open,
    /// Recovering — a limited number of probe requests are allowed through.
    HalfOpen,
}

/// Circuit breaker configuration.
#[derive(Clone)]
pub struct CbConfig {
    /// How many consecutive failures open the breaker.
    pub max_failures: usize,
    /// How long to stay open before moving to half-open.
    pub timeout: Duration,
    /// Max probe calls allowed in half-open state before closing.
    pub half_open_max_calls: usize,
    /// Optional closure invoked on every state transition `(from, to)`.
    pub on_state_change: Option<Arc<dyn Fn(CbState, CbState) + Send + Sync>>,
    /// Name for logging.
    pub name: String,
}

impl std::fmt::Debug for CbConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CbConfig")
            .field("max_failures", &self.max_failures)
            .field("timeout", &self.timeout)
            .field("half_open_max_calls", &self.half_open_max_calls)
            .field("on_state_change", &self.on_state_change.as_ref().map(|_| "<fn>"))
            .field("name", &self.name)
            .finish()
    }
}

impl Default for CbConfig {
    fn default() -> Self {
        Self {
            max_failures: 5,
            timeout: Duration::from_secs(30),
            half_open_max_calls: 3,
            on_state_change: None,
            name: "cb".to_string(),
        }
    }
}

impl CbConfig {
    /// Create a new config with the given `name` and all other fields at defaults.
    pub fn new(name: impl Into<String>) -> Self {
        Self { name: name.into(), ..Default::default() }
    }

    /// Set the consecutive failure threshold that trips the breaker.
    #[must_use]
    pub fn with_max_failures(mut self, n: usize) -> Self {
        self.max_failures = n;
        self
    }

    /// Set how long the breaker stays open before entering half-open.
    #[must_use]
    pub fn with_timeout(mut self, t: Duration) -> Self {
        self.timeout = t;
        self
    }

    /// Set the maximum number of probe calls allowed in the half-open state.
    #[must_use]
    pub fn with_half_open_max_calls(mut self, n: usize) -> Self {
        self.half_open_max_calls = n;
        self
    }

    /// Register a closure invoked on every state transition `(from, to)`.
    #[must_use]
    pub fn with_on_state_change(
        mut self,
        f: impl Fn(CbState, CbState) + Send + Sync + 'static,
    ) -> Self {
        self.on_state_change = Some(Arc::new(f));
        self
    }
}

// ── Internal mutable state (held under parking_lot::Mutex) ─────────────────

struct Inner {
    state: CbState,
    failures: usize,
    successes: usize,
    half_open_calls: usize,
    last_failure: Option<Instant>,
}

impl Inner {
    fn new() -> Self {
        Self {
            state: CbState::Closed,
            failures: 0,
            successes: 0,
            half_open_calls: 0,
            last_failure: None,
        }
    }
}

/// Asynchronous circuit breaker.
///
/// `parking_lot::Mutex` is used for state; it is never held across `.await`
/// points, so async code remains efficient.
///
/// # State machine
///
/// ```text
/// Closed ──(>max_failures)──► Open ──(timeout elapsed)──► HalfOpen
///   ▲                                                          │
///   └──────────────(probes succeed)───────────────────────────┘
///                         │
///               (probe fails)──► Open
/// ```
#[derive(Clone)]
pub struct CircuitBreaker {
    inner: Arc<Mutex<Inner>>,
    config: Arc<CbConfig>,
}

impl CircuitBreaker {
    /// Create a new [`CircuitBreaker`] with the given configuration.
    pub fn new(config: CbConfig) -> Self {
        Self {
            inner: Arc::new(Mutex::new(Inner::new())),
            config: Arc::new(config),
        }
    }

    /// Return the current observable state of the breaker.
    pub fn state(&self) -> CbState {
        self.inner.lock().state
    }

    /// Return the current consecutive failure count.
    pub fn failures(&self) -> usize {
        self.inner.lock().failures
    }

    /// Reset the breaker to closed state with all counters zeroed.
    pub fn reset(&self) {
        let mut inner = self.inner.lock();
        *inner = Inner::new();
    }

    /// Execute `f` through the circuit breaker.
    ///
    /// - `Closed` / `HalfOpen` (within probe limit): calls `f`
    /// - `Open` (timeout not elapsed): returns `AppError::service_unavailable`
    pub async fn execute<F, Fut, T>(&self, f: F) -> AppResult<T>
    where
        F: FnOnce() -> Fut,
        Fut: std::future::Future<Output = AppResult<T>>,
    {
        // ── pre-call: check state (brief lock, no await) ────────────────
        let can_proceed = {
            let mut inner = self.inner.lock();
            match inner.state {
                CbState::Closed => true,
                CbState::Open => {
                    if inner
                        .last_failure
                        .map(|t| t.elapsed() >= self.config.timeout)
                        .unwrap_or(false)
                    {
                        self.transition(&mut inner, CbState::HalfOpen);
                        inner.half_open_calls = 0;
                        inner.successes = 0;
                        true
                    } else {
                        false
                    }
                }
                CbState::HalfOpen => {
                    if inner.half_open_calls < self.config.half_open_max_calls {
                        inner.half_open_calls += 1;
                        true
                    } else {
                        false
                    }
                }
            }
        };

        if !can_proceed {
            return Err(AppError::service_unavailable(&self.config.name)
                .with_detail("circuit_breaker_state", "open"));
        }

        // ── call (outside lock, across await) ───────────────────────────
        let result = f().await;

        // ── post-call: update state (brief lock, no await) ──────────────
        {
            let mut inner = self.inner.lock();
            match &result {
                Ok(_) => {
                    match inner.state {
                        CbState::HalfOpen => {
                            inner.successes += 1;
                            if inner.successes >= self.config.half_open_max_calls {
                                inner.failures = 0;
                                self.transition(&mut inner, CbState::Closed);
                            }
                        }
                        CbState::Closed => {
                            inner.failures = 0;
                        }
                        CbState::Open => {}
                    }
                }
                Err(_) => {
                    inner.failures += 1;
                    inner.last_failure = Some(Instant::now());
                    match inner.state {
                        CbState::Closed => {
                            if inner.failures >= self.config.max_failures {
                                self.transition(&mut inner, CbState::Open);
                            }
                        }
                        CbState::HalfOpen => {
                            self.transition(&mut inner, CbState::Open);
                        }
                        CbState::Open => {}
                    }
                }
            }
        }

        result
    }

    fn transition(&self, inner: &mut Inner, to: CbState) {
        let from = inner.state;
        inner.state = to;
        tracing::debug!(
            cb = %self.config.name,
            from = ?from,
            to = ?to,
            "circuit breaker state transition"
        );
        if let Some(cb) = &self.config.on_state_change {
            cb(from, to);
        }
    }
}

#[cfg(test)]
mod tests {
    use rskit_errors::{AppError, ErrorCode};
    use super::*;

    fn make_cb(max_failures: usize) -> CircuitBreaker {
        CircuitBreaker::new(
            CbConfig::new("test-cb").with_max_failures(max_failures),
        )
    }

    #[tokio::test]
    async fn execute_passes_through_success() {
        let cb = make_cb(3);
        let result = cb.execute(|| async { Ok::<i32, AppError>(7) }).await;
        assert_eq!(result.unwrap(), 7);
        assert_eq!(cb.state(), CbState::Closed);
    }

    #[tokio::test]
    async fn state_is_closed_initially() {
        let cb = make_cb(3);
        assert_eq!(cb.state(), CbState::Closed);
    }

    #[tokio::test]
    async fn execute_opens_after_max_failures_consecutive_failures() {
        let cb = make_cb(3);

        for _ in 0..3 {
            let _ = cb.execute(|| async {
                Err::<i32, AppError>(AppError::new(ErrorCode::ConnectionFailed, "fail"))
            }).await;
        }

        assert_eq!(cb.state(), CbState::Open);
    }

    #[tokio::test]
    async fn execute_rejects_immediately_when_open() {
        let cb = make_cb(2);

        // Trip the breaker
        for _ in 0..2 {
            let _ = cb.execute(|| async {
                Err::<i32, AppError>(AppError::new(ErrorCode::Internal, "fail"))
            }).await;
        }
        assert_eq!(cb.state(), CbState::Open);

        // Next call should be rejected without calling the function
        let result = cb.execute(|| async { Ok::<i32, AppError>(42) }).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn failures_resets_to_zero_on_success_in_closed_state() {
        let cb = make_cb(5);

        // Cause some failures but not enough to open
        for _ in 0..2 {
            let _ = cb.execute(|| async {
                Err::<i32, AppError>(AppError::new(ErrorCode::Internal, "fail"))
            }).await;
        }
        assert_eq!(cb.failures(), 2);

        // A success should reset failures
        let _ = cb.execute(|| async { Ok::<i32, AppError>(1) }).await;
        assert_eq!(cb.failures(), 0);
        assert_eq!(cb.state(), CbState::Closed);
    }

    #[tokio::test]
    async fn reset_restores_closed_state() {
        let cb = make_cb(2);

        for _ in 0..2 {
            let _ = cb.execute(|| async {
                Err::<i32, AppError>(AppError::new(ErrorCode::Internal, "fail"))
            }).await;
        }
        assert_eq!(cb.state(), CbState::Open);

        cb.reset();
        assert_eq!(cb.state(), CbState::Closed);
        assert_eq!(cb.failures(), 0);
    }
}
