use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use std::time::Duration;

use rskit_component::{
    Component, Health, HealthStatus, LazyComponent, Registry, RegistryConfig, State,
};
use rskit_errors::{AppError, ErrorCode};
use tokio_util::sync::CancellationToken;

#[derive(Default)]
struct Counts {
    start: AtomicUsize,
    stop: AtomicUsize,
    health: AtomicUsize,
}

struct CountingComponent {
    name: &'static str,
    counts: Arc<Counts>,
    start: StartBehavior,
    stop: StopBehavior,
}

enum StartBehavior {
    Ok,
    Fail,
    Panic,
    Sleep(Duration),
}

enum StopBehavior {
    Ok,
    Fail,
    Sleep(Duration),
}

impl CountingComponent {
    fn new(name: &'static str, counts: Arc<Counts>) -> Self {
        Self {
            name,
            counts,
            start: StartBehavior::Ok,
            stop: StopBehavior::Ok,
        }
    }

    fn with_start(mut self, start: StartBehavior) -> Self {
        self.start = start;
        self
    }

    fn with_stop(mut self, stop: StopBehavior) -> Self {
        self.stop = stop;
        self
    }
}

#[async_trait::async_trait]
impl Component for CountingComponent {
    fn name(&self) -> &str {
        self.name
    }

    async fn start(&self) -> rskit_errors::AppResult<()> {
        self.counts.start.fetch_add(1, Ordering::SeqCst);
        match self.start {
            StartBehavior::Ok => Ok(()),
            StartBehavior::Fail => Err(AppError::service_unavailable(self.name)),
            StartBehavior::Panic => panic!("{} start panicked", self.name),
            StartBehavior::Sleep(duration) => {
                tokio::time::sleep(duration).await;
                Ok(())
            }
        }
    }

    async fn stop(&self) -> rskit_errors::AppResult<()> {
        self.counts.stop.fetch_add(1, Ordering::SeqCst);
        match self.stop {
            StopBehavior::Ok => Ok(()),
            StopBehavior::Fail => Err(AppError::service_unavailable(self.name)),
            StopBehavior::Sleep(duration) => {
                tokio::time::sleep(duration).await;
                Ok(())
            }
        }
    }

    fn health(&self) -> Health {
        self.counts.health.fetch_add(1, Ordering::SeqCst);
        Health::healthy(self.name)
    }
}

#[tokio::test]
async fn lazy_component_defers_factory_and_reuses_inner_component() {
    let factory_calls = Arc::new(AtomicUsize::new(0));
    let counts = Arc::new(Counts::default());
    let lazy = LazyComponent::new("lazy", {
        let factory_calls = factory_calls.clone();
        let counts = counts.clone();
        move || {
            factory_calls.fetch_add(1, Ordering::SeqCst);
            Arc::new(CountingComponent::new("inner", counts.clone())) as Arc<dyn Component>
        }
    });

    assert_eq!(lazy.name(), "lazy");
    assert!(lazy.health().is_healthy());
    assert_eq!(factory_calls.load(Ordering::SeqCst), 0);

    lazy.start().await.unwrap();
    lazy.start().await.unwrap();
    assert_eq!(factory_calls.load(Ordering::SeqCst), 1);
    assert_eq!(counts.start.load(Ordering::SeqCst), 2);

    assert_eq!(lazy.health().name, "inner");
    assert_eq!(counts.health.load(Ordering::SeqCst), 1);

    lazy.stop().await.unwrap();
    assert_eq!(counts.stop.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn lazy_component_stop_before_start_is_noop() {
    let lazy = LazyComponent::new("lazy-empty", || {
        Arc::new(CountingComponent::new(
            "unused",
            Arc::new(Counts::default()),
        )) as Arc<dyn Component>
    });

    lazy.stop().await.unwrap();
    assert_eq!(lazy.health().name, "lazy-empty");
}

#[tokio::test]
async fn registry_tracks_counts_health_and_stop_failures() {
    let counts = Arc::new(Counts::default());
    let mut registry = Registry::new();
    assert!(registry.is_empty());
    registry.register(Arc::new(
        CountingComponent::new("svc", counts.clone()).with_stop(StopBehavior::Fail),
    ));

    assert_eq!(registry.len(), 1);
    assert_eq!(registry.state("svc"), Some(State::Created));
    registry.start_all().await.unwrap();
    assert_eq!(registry.state("svc"), Some(State::Running));

    let health = registry.health_all();
    assert_eq!(health[0].status, HealthStatus::Healthy);
    assert_eq!(counts.health.load(Ordering::SeqCst), 1);

    let err = registry.stop_all().await.unwrap_err();
    assert_eq!(err.code(), ErrorCode::Internal);
    assert_eq!(registry.state("svc"), Some(State::Failed));
}

#[tokio::test]
async fn start_failure_reports_rollback_stop_failures() {
    let ok_counts = Arc::new(Counts::default());
    let fail_counts = Arc::new(Counts::default());
    let mut registry = Registry::new();
    registry.register(Arc::new(
        CountingComponent::new("started", ok_counts.clone()).with_stop(StopBehavior::Fail),
    ));
    registry.register(Arc::new(
        CountingComponent::new("failing", fail_counts).with_start(StartBehavior::Fail),
    ));

    let err = registry.start_all().await.unwrap_err();
    assert_eq!(err.code(), ErrorCode::ServiceUnavailable);
    assert!(err.to_string().contains("rollback failures"));
    assert_eq!(registry.state("started"), Some(State::Failed));
    assert_eq!(registry.state("failing"), Some(State::Failed));
    assert_eq!(ok_counts.stop.load(Ordering::SeqCst), 1);
}

#[tokio::test(start_paused = true)]
async fn start_timeout_attempts_cleanup_and_reports_stop_failure() {
    let counts = Arc::new(Counts::default());
    let mut registry = Registry::with_config(RegistryConfig {
        start_timeout: Duration::from_secs(1),
        stop_timeout: Duration::from_secs(1),
        ..RegistryConfig::default()
    });
    registry.register(Arc::new(
        CountingComponent::new("slow", counts.clone())
            .with_start(StartBehavior::Sleep(Duration::from_secs(60)))
            .with_stop(StopBehavior::Fail),
    ));

    let err = registry.start_all().await.unwrap_err();
    assert_eq!(err.code(), ErrorCode::Timeout);
    assert_eq!(registry.state("slow"), Some(State::Failed));
    assert_eq!(counts.stop.load(Ordering::SeqCst), 1);
}

#[tokio::test(start_paused = true)]
async fn stop_all_detailed_reports_timeout() {
    let mut registry = Registry::with_config(RegistryConfig {
        stop_timeout: Duration::from_secs(1),
        ..RegistryConfig::default()
    });
    registry.register(Arc::new(
        CountingComponent::new("slow-stop", Arc::new(Counts::default()))
            .with_stop(StopBehavior::Sleep(Duration::from_secs(60))),
    ));
    registry.start_all().await.unwrap();

    let results = registry.stop_all_detailed().await;
    assert_eq!(results.len(), 1);
    assert_eq!(results[0].name, "slow-stop");
    assert_eq!(
        results[0].error.as_ref().map(AppError::code),
        Some(ErrorCode::Timeout)
    );
    assert_eq!(registry.state("slow-stop"), Some(State::Failed));
}

#[tokio::test]
async fn concurrent_start_handles_empty_success_and_pre_cancelled_runs() {
    let empty = Registry::new();
    empty
        .start_all_concurrent(CancellationToken::new())
        .await
        .unwrap();

    let counts = Arc::new(Counts::default());
    let mut registry = Registry::with_config(RegistryConfig {
        concurrency: 0,
        ..RegistryConfig::default()
    });
    registry.register(Arc::new(CountingComponent::new(
        "cancelled",
        counts.clone(),
    )));

    let cancel = CancellationToken::new();
    cancel.cancel();
    registry.start_all_concurrent(cancel).await.unwrap();
    assert_eq!(registry.state("cancelled"), Some(State::Created));
    assert_eq!(counts.start.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn concurrent_start_success_updates_running_state() {
    let counts = Arc::new(Counts::default());
    let mut registry = Registry::with_config(RegistryConfig {
        concurrency: 0,
        ..RegistryConfig::default()
    });
    registry.register(Arc::new(CountingComponent::new("a", counts.clone())));
    registry.register(Arc::new(CountingComponent::new("b", counts.clone())));

    registry
        .start_all_concurrent(CancellationToken::new())
        .await
        .unwrap();

    assert_eq!(registry.state("a"), Some(State::Running));
    assert_eq!(registry.state("b"), Some(State::Running));
    assert_eq!(counts.start.load(Ordering::SeqCst), 2);
}

struct BlockingStartComponent {
    entered: Arc<tokio::sync::Notify>,
    release: Arc<tokio::sync::Notify>,
}

#[async_trait::async_trait]
impl Component for BlockingStartComponent {
    fn name(&self) -> &str {
        "blocking"
    }

    async fn start(&self) -> rskit_errors::AppResult<()> {
        self.entered.notify_waiters();
        self.release.notified().await;
        Ok(())
    }

    async fn stop(&self) -> rskit_errors::AppResult<()> {
        Ok(())
    }

    fn health(&self) -> Health {
        Health::healthy(self.name())
    }
}

#[tokio::test]
async fn start_all_rejects_component_already_starting() {
    let entered = Arc::new(tokio::sync::Notify::new());
    let release = Arc::new(tokio::sync::Notify::new());
    let mut registry = Registry::new();
    registry.register(Arc::new(BlockingStartComponent {
        entered: entered.clone(),
        release: release.clone(),
    }));
    let registry = Arc::new(registry);

    let starting = {
        let registry = registry.clone();
        tokio::spawn(async move { registry.start_all().await })
    };
    entered.notified().await;

    let err = registry.start_all().await.unwrap_err();
    assert_eq!(err.code(), ErrorCode::Conflict);
    assert!(err.to_string().contains("cannot start from state starting"));

    release.notify_waiters();
    starting.await.unwrap().unwrap();
}

#[tokio::test(start_paused = true)]
async fn start_timeout_reports_cleanup_stop_timeout() {
    let mut registry = Registry::with_config(RegistryConfig {
        start_timeout: Duration::from_secs(1),
        stop_timeout: Duration::from_secs(1),
        ..RegistryConfig::default()
    });
    registry.register(Arc::new(
        CountingComponent::new("slow-cleanup", Arc::new(Counts::default()))
            .with_start(StartBehavior::Sleep(Duration::from_secs(60)))
            .with_stop(StopBehavior::Sleep(Duration::from_secs(60))),
    ));

    let err = registry.start_all().await.unwrap_err();
    assert_eq!(err.code(), ErrorCode::Timeout);
    assert_eq!(registry.state("slow-cleanup"), Some(State::Failed));
}

#[tokio::test(start_paused = true)]
async fn start_failure_reports_rollback_stop_timeout() {
    let mut registry = Registry::with_config(RegistryConfig {
        stop_timeout: Duration::from_secs(1),
        ..RegistryConfig::default()
    });
    registry.register(Arc::new(
        CountingComponent::new("started", Arc::new(Counts::default()))
            .with_stop(StopBehavior::Sleep(Duration::from_secs(60))),
    ));
    registry.register(Arc::new(
        CountingComponent::new("failing", Arc::new(Counts::default()))
            .with_start(StartBehavior::Fail),
    ));

    let err = registry.start_all().await.unwrap_err();
    assert_eq!(err.code(), ErrorCode::ServiceUnavailable);
    assert!(err.to_string().contains("rollback failures"));
    assert_eq!(registry.state("started"), Some(State::Failed));
}

#[tokio::test(start_paused = true)]
async fn concurrent_start_timeout_marks_component_failed() {
    let mut registry = Registry::with_config(RegistryConfig {
        start_timeout: Duration::from_secs(1),
        ..RegistryConfig::default()
    });
    registry.register(Arc::new(
        CountingComponent::new("slow-concurrent", Arc::new(Counts::default()))
            .with_start(StartBehavior::Sleep(Duration::from_secs(60))),
    ));

    let err = registry
        .start_all_concurrent(CancellationToken::new())
        .await
        .unwrap_err();
    assert_eq!(err.code(), ErrorCode::Timeout);
    assert_eq!(registry.state("slow-concurrent"), Some(State::Failed));
}

#[tokio::test]
async fn concurrent_start_converts_join_panics_to_internal_errors() {
    let mut registry = Registry::new();
    registry.register(Arc::new(
        CountingComponent::new("panic", Arc::new(Counts::default()))
            .with_start(StartBehavior::Panic),
    ));

    let err = registry
        .start_all_concurrent(CancellationToken::new())
        .await
        .unwrap_err();
    assert_eq!(err.code(), ErrorCode::Internal);
    assert_eq!(registry.state("panic"), Some(State::Starting));
}
