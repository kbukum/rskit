use std::sync::Arc;
use std::time::Duration;

use parking_lot::Mutex;
use rskit_errors::{AppError, AppResult};
use tokio::time::Instant;

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
            .field(
                "on_state_change",
                &self.on_state_change.as_ref().map(|_| "<fn>"),
            )
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
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            ..Default::default()
        }
    }

    /// Validate that breaker thresholds are bounded and non-zero.
    ///
    /// # Errors
    /// Returns an error when thresholds would make the breaker permanently unusable.
    pub fn validate(&self) -> AppResult<()> {
        if self.max_failures == 0 {
            return Err(AppError::invalid_input(
                "max_failures",
                "circuit breaker failure threshold must be greater than zero",
            ));
        }
        if self.half_open_max_calls == 0 {
            return Err(AppError::invalid_input(
                "half_open_max_calls",
                "half-open probe limit must be greater than zero",
            ));
        }
        Ok(())
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
    ///
    /// # Errors
    /// Returns an error when the configuration is invalid.
    pub fn new(config: CbConfig) -> AppResult<Self> {
        config.validate()?;
        Ok(Self {
            inner: Arc::new(Mutex::new(Inner::new())),
            config: Arc::new(config),
        })
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
        let mut transition = None;
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
                        transition = self.transition(&mut inner, CbState::HalfOpen);
                        inner.half_open_calls = 1;
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
        self.notify_transition(transition);

        if !can_proceed {
            return Err(AppError::service_unavailable(&self.config.name)
                .with_detail("circuit_breaker_state", "open"));
        }

        // ── call (outside lock, across await) ───────────────────────────
        let result = f().await;

        // ── post-call: update state (brief lock, no await) ──────────────
        {
            let mut inner = self.inner.lock();
            let mut transition = None;
            match &result {
                Ok(_) => match inner.state {
                    CbState::HalfOpen => {
                        inner.successes += 1;
                        if inner.successes >= self.config.half_open_max_calls {
                            inner.failures = 0;
                            transition = self.transition(&mut inner, CbState::Closed);
                        }
                    }
                    CbState::Closed => {
                        inner.failures = 0;
                    }
                    CbState::Open => {}
                },
                Err(_) => {
                    inner.failures += 1;
                    inner.last_failure = Some(Instant::now());
                    match inner.state {
                        CbState::Closed => {
                            if inner.failures >= self.config.max_failures {
                                transition = self.transition(&mut inner, CbState::Open);
                            }
                        }
                        CbState::HalfOpen => {
                            transition = self.transition(&mut inner, CbState::Open);
                        }
                        CbState::Open => {}
                    }
                }
            }
            drop(inner);
            self.notify_transition(transition);
        }

        result
    }

    fn transition(&self, inner: &mut Inner, to: CbState) -> Option<(CbState, CbState)> {
        let from = inner.state;
        if from == to {
            return None;
        }
        inner.state = to;
        tracing::debug!(
            cb = %self.config.name,
            from = ?from,
            to = ?to,
            "circuit breaker state transition"
        );
        Some((from, to))
    }

    fn notify_transition(&self, transition: Option<(CbState, CbState)>) {
        let Some((from, to)) = transition else {
            return;
        };
        if let Some(cb) = &self.config.on_state_change {
            cb(from, to);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rskit_errors::{AppError, ErrorCode};

    fn make_cb(max_failures: usize) -> CircuitBreaker {
        CircuitBreaker::new(CbConfig::new("test-cb").with_max_failures(max_failures)).unwrap()
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
            let _ = cb
                .execute(|| async {
                    Err::<i32, AppError>(AppError::new(ErrorCode::ConnectionFailed, "fail"))
                })
                .await;
        }

        assert_eq!(cb.state(), CbState::Open);
    }

    #[tokio::test]
    async fn execute_rejects_immediately_when_open() {
        let cb = make_cb(2);

        // Trip the breaker
        for _ in 0..2 {
            let _ = cb
                .execute(|| async {
                    Err::<i32, AppError>(AppError::new(ErrorCode::Internal, "fail"))
                })
                .await;
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
            let _ = cb
                .execute(|| async {
                    Err::<i32, AppError>(AppError::new(ErrorCode::Internal, "fail"))
                })
                .await;
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
            let _ = cb
                .execute(|| async {
                    Err::<i32, AppError>(AppError::new(ErrorCode::Internal, "fail"))
                })
                .await;
        }
        assert_eq!(cb.state(), CbState::Open);

        cb.reset();
        assert_eq!(cb.state(), CbState::Closed);
        assert_eq!(cb.failures(), 0);
    }

    #[test]
    fn new_rejects_invalid_thresholds() {
        assert!(CircuitBreaker::new(CbConfig::new("zero-failures").with_max_failures(0)).is_err());
        assert!(
            CircuitBreaker::new(CbConfig::new("zero-probes").with_half_open_max_calls(0)).is_err()
        );
    }

    #[tokio::test(start_paused = true)]
    async fn open_breaker_moves_to_half_open_with_virtual_time() {
        let cb = CircuitBreaker::new(
            CbConfig::new("virtual")
                .with_max_failures(1)
                .with_timeout(Duration::from_secs(5))
                .with_half_open_max_calls(1),
        )
        .unwrap();

        let _ = cb
            .execute(|| async { Err::<(), AppError>(AppError::new(ErrorCode::Internal, "fail")) })
            .await;
        assert_eq!(cb.state(), CbState::Open);

        tokio::time::advance(Duration::from_secs(5)).await;
        let result = cb.execute(|| async { Ok::<_, AppError>(()) }).await;

        assert!(result.is_ok());
        assert_eq!(cb.state(), CbState::Closed);
    }

    #[tokio::test]
    async fn open_to_half_open_transition_consumes_probe_slot() {
        let cb = CircuitBreaker::new(
            CbConfig::new("probe-limit")
                .with_max_failures(1)
                .with_timeout(Duration::ZERO)
                .with_half_open_max_calls(1),
        )
        .unwrap();

        let _ = cb
            .execute(|| async { Err::<(), AppError>(AppError::new(ErrorCode::Internal, "fail")) })
            .await;
        assert_eq!(cb.state(), CbState::Open);

        let (started_tx, started_rx) = tokio::sync::oneshot::channel();
        let release = Arc::new(tokio::sync::Notify::new());
        let first_probe = {
            let cb = cb.clone();
            let release = Arc::clone(&release);
            tokio::spawn(async move {
                cb.execute(|| async move {
                    started_tx.send(()).unwrap();
                    release.notified().await;
                    Ok::<_, AppError>(())
                })
                .await
            })
        };
        started_rx.await.unwrap();

        let second_probe = cb.execute(|| async { Ok::<_, AppError>(()) }).await;
        assert!(second_probe.is_err());

        release.notify_one();
        assert!(first_probe.await.unwrap().is_ok());
        assert_eq!(cb.state(), CbState::Closed);
    }
}
