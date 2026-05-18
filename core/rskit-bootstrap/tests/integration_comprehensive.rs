use std::sync::{
    Arc,
    atomic::{AtomicBool, AtomicUsize, Ordering},
};
use std::time::Duration;

use parking_lot::Mutex;

use async_trait::async_trait;
use rskit_bootstrap::component::Component;
use rskit_bootstrap::{
    AppBuilder, CancellationToken, Health, HealthStatus, LazyComponent, Registry, RegistryConfig,
};
use rskit_config::{AppConfig, ServiceConfig};
use rskit_errors::{AppError, AppResult};

// ── Test config ──────────────────────────────────────────────────────────────

#[derive(Debug, Default, serde::Deserialize, rskit_validation::Validate)]
struct TestCfg {
    #[serde(default)]
    service: ServiceConfig,
}

impl rskit_config::AppConfig for TestCfg {
    fn apply_defaults(&mut self) {}
    fn service_config(&self) -> &ServiceConfig {
        &self.service
    }
}

// ── Mock components ──────────────────────────────────────────────────────────

struct MockComponent {
    name: String,
    start_count: Arc<AtomicUsize>,
    stop_count: Arc<AtomicUsize>,
    healthy: bool,
    fail_on_start: Option<String>,
    fail_on_stop: Option<String>,
    start_order: Option<Arc<Mutex<Vec<String>>>>,
    stop_order: Option<Arc<Mutex<Vec<String>>>>,
    start_delay: Option<Duration>,
}

impl MockComponent {
    fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            start_count: Arc::new(AtomicUsize::new(0)),
            stop_count: Arc::new(AtomicUsize::new(0)),
            healthy: true,
            fail_on_start: None,
            fail_on_stop: None,
            start_order: None,
            stop_order: None,
            start_delay: None,
        }
    }

    fn with_fail_on_start(mut self, msg: impl Into<String>) -> Self {
        self.fail_on_start = Some(msg.into());
        self
    }

    fn with_fail_on_stop(mut self, msg: impl Into<String>) -> Self {
        self.fail_on_stop = Some(msg.into());
        self
    }

    fn with_start_order(mut self, order: Arc<Mutex<Vec<String>>>) -> Self {
        self.start_order = Some(order);
        self
    }

    fn with_stop_order(mut self, order: Arc<Mutex<Vec<String>>>) -> Self {
        self.stop_order = Some(order);
        self
    }

    fn with_unhealthy(mut self) -> Self {
        self.healthy = false;
        self
    }

    fn with_start_delay(mut self, delay: Duration) -> Self {
        self.start_delay = Some(delay);
        self
    }
}

#[async_trait]
impl Component for MockComponent {
    fn name(&self) -> &str {
        &self.name
    }

    async fn start(&self) -> AppResult<()> {
        if let Some(delay) = self.start_delay {
            tokio::time::sleep(delay).await;
        }
        if let Some(msg) = &self.fail_on_start {
            return Err(AppError::service_unavailable(msg.clone()));
        }
        self.start_count.fetch_add(1, Ordering::SeqCst);
        if let Some(order) = &self.start_order {
            order.lock().push(self.name.clone());
        }
        Ok(())
    }

    async fn stop(&self) -> AppResult<()> {
        if let Some(msg) = &self.fail_on_stop {
            return Err(AppError::service_unavailable(msg.clone()));
        }
        self.stop_count.fetch_add(1, Ordering::SeqCst);
        if let Some(order) = &self.stop_order {
            order.lock().push(self.name.clone());
        }
        Ok(())
    }

    fn health(&self) -> Health {
        if self.healthy {
            Health::healthy(&self.name)
        } else {
            Health::unhealthy(&self.name, "mock unhealthy")
        }
    }
}

// ── 1. AppBuilder construction and validation ────────────────────────────────

#[tokio::test]
async fn app_builder_default_config() {
    let cfg = TestCfg::default();
    let app = AppBuilder::new(cfg).build();
    assert!(app.is_ok(), "default config should build successfully");
}

#[tokio::test]
async fn app_builder_with_graceful_timeout() {
    let cfg = TestCfg::default();
    let app = AppBuilder::new(cfg)
        .with_graceful_timeout(Duration::from_secs(5))
        .build();
    assert!(app.is_ok());
}

#[tokio::test]
async fn app_builder_with_component() {
    let cfg = TestCfg::default();
    let comp = Arc::new(MockComponent::new("db"));
    let app = AppBuilder::new(cfg).with_component(comp).build();
    assert!(app.is_ok());
}

#[tokio::test]
async fn app_builder_with_multiple_components() {
    let cfg = TestCfg::default();
    let app = AppBuilder::new(cfg)
        .with_component(Arc::new(MockComponent::new("db")))
        .with_component(Arc::new(MockComponent::new("cache")))
        .with_component(Arc::new(MockComponent::new("kafka")))
        .build();
    assert!(app.is_ok());
}

// ── 2. Component registration and ordering ───────────────────────────────────

#[tokio::test]
async fn registry_start_order_matches_registration() {
    let order: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));

    let mut reg = Registry::new();
    reg.register(Arc::new(
        MockComponent::new("alpha").with_start_order(order.clone()),
    ));
    reg.register(Arc::new(
        MockComponent::new("beta").with_start_order(order.clone()),
    ));
    reg.register(Arc::new(
        MockComponent::new("gamma").with_start_order(order.clone()),
    ));

    reg.start_all().await.unwrap();
    let started = order.lock();
    assert_eq!(*started, vec!["alpha", "beta", "gamma"]);
}

#[tokio::test]
async fn registry_stop_order_is_lifo() {
    let stop_order: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));

    let mut reg = Registry::new();
    reg.register(Arc::new(
        MockComponent::new("first").with_stop_order(stop_order.clone()),
    ));
    reg.register(Arc::new(
        MockComponent::new("second").with_stop_order(stop_order.clone()),
    ));
    reg.register(Arc::new(
        MockComponent::new("third").with_stop_order(stop_order.clone()),
    ));

    reg.start_all().await.unwrap();
    reg.stop_all().await.unwrap();

    let order = stop_order.lock();
    assert_eq!(*order, vec!["third", "second", "first"]);
}

// ── 3. Lifecycle hook ordering ───────────────────────────────────────────────

#[tokio::test]
async fn lifecycle_hooks_execute_in_order() {
    let order: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let cfg = TestCfg::default();

    let o1 = order.clone();
    let o2 = order.clone();
    let o3 = order.clone();
    let o4 = order.clone();

    let app = AppBuilder::new(cfg)
        .build()
        .unwrap()
        .before_start(move |_tok| {
            let o = o1.clone();
            async move {
                o.lock().push("configure".into());
                Ok(())
            }
        })
        .before_start(move |_tok| {
            let o = o2.clone();
            async move {
                o.lock().push("start".into());
                Ok(())
            }
        })
        .after_start(move |_tok| {
            let o = o3.clone();
            async move {
                o.lock().push("ready".into());
                Ok(())
            }
        })
        .before_stop(move |_tok| {
            let o = o4.clone();
            async move {
                o.lock().push("stop".into());
                Ok(())
            }
        });

    let result = app.run_task(|_cfg, _cancel| async move { Ok(()) }).await;
    assert!(result.is_ok());

    let executed = order.lock();
    assert_eq!(
        *executed,
        vec!["configure", "start", "ready", "stop"],
        "run_task: configure → start → ready → task → stop"
    );
}

// ── 4. Hook error propagation ────────────────────────────────────────────────

#[tokio::test]
async fn configure_hook_error_propagates() {
    let cfg = TestCfg::default();
    let app = AppBuilder::new(cfg)
        .build()
        .unwrap()
        .before_start(|_tok| async { Err(AppError::service_unavailable("configure boom")) });

    let result = app.run_task(|_cfg, _cancel| async move { Ok(()) }).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn start_hook_error_propagates() {
    let cfg = TestCfg::default();
    let app = AppBuilder::new(cfg)
        .build()
        .unwrap()
        .before_start(|_tok| async { Err(AppError::service_unavailable("start boom")) });

    let result = app.run_task(|_cfg, _cancel| async move { Ok(()) }).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn stop_hook_error_propagates() {
    let cfg = TestCfg::default();
    let app = AppBuilder::new(cfg)
        .build()
        .unwrap()
        .before_stop(|_tok| async { Err(AppError::service_unavailable("stop boom")) });

    let result = app.run_task(|_cfg, _cancel| async move { Ok(()) }).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn hook_error_stops_subsequent_hooks() {
    let second_called = Arc::new(AtomicBool::new(false));
    let second_called_clone = second_called.clone();

    let cfg = TestCfg::default();
    let app = AppBuilder::new(cfg)
        .build()
        .unwrap()
        .before_start(|_tok| async { Err(AppError::service_unavailable("first fails")) })
        .before_start(move |_tok| {
            let called = second_called_clone.clone();
            async move {
                called.store(true, Ordering::SeqCst);
                Ok(())
            }
        });

    let _ = app.run_task(|_cfg, _cancel| async move { Ok(()) }).await;
    assert!(
        !second_called.load(Ordering::SeqCst),
        "second hook should not run after first fails"
    );
}

// ── 5. start_all_concurrent with cancellation ────────────────────────────────

#[tokio::test]
async fn start_all_concurrent_succeeds() {
    let config = RegistryConfig {
        concurrency: 0, // unlimited
        start_timeout: Duration::from_secs(5),
        stop_timeout: Duration::from_secs(5),
    };
    let mut reg = Registry::with_config(config);

    let start_order: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    for name in ["a", "b", "c"] {
        reg.register(Arc::new(
            MockComponent::new(name).with_start_order(start_order.clone()),
        ));
    }

    let cancel = CancellationToken::new();
    let result = reg.start_all_concurrent(cancel).await;
    assert!(result.is_ok());

    let started = start_order.lock();
    assert_eq!(started.len(), 3, "all 3 should have started");
}

#[tokio::test]
async fn start_all_concurrent_with_cancellation() {
    let config = RegistryConfig {
        concurrency: 1, // sequential for predictability
        start_timeout: Duration::from_secs(5),
        stop_timeout: Duration::from_secs(5),
    };
    let mut reg = Registry::with_config(config);

    // First component is slow enough to be cancelled
    reg.register(Arc::new(
        MockComponent::new("slow").with_start_delay(Duration::from_secs(10)),
    ));
    reg.register(Arc::new(MockComponent::new("fast")));

    let cancel = CancellationToken::new();
    let cancel_clone = cancel.clone();

    // Cancel after a short delay
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(50)).await;
        cancel_clone.cancel();
    });

    let result = reg.start_all_concurrent(cancel).await;
    // Should complete (cancelled) without hanging
    assert!(result.is_ok(), "cancellation should return Ok");
}

#[tokio::test]
async fn start_all_concurrent_error_propagates() {
    let config = RegistryConfig {
        concurrency: 2,
        start_timeout: Duration::from_secs(5),
        stop_timeout: Duration::from_secs(5),
    };
    let mut reg = Registry::with_config(config);

    reg.register(Arc::new(MockComponent::new("ok")));
    reg.register(Arc::new(
        MockComponent::new("fail").with_fail_on_start("concurrent boom"),
    ));

    let cancel = CancellationToken::new();
    let result = reg.start_all_concurrent(cancel).await;
    assert!(result.is_err());
}

// ── 6. stop_all error collection ─────────────────────────────────────────────

#[tokio::test]
async fn stop_all_continues_on_error() {
    let stop_order: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));

    let mut reg = Registry::new();
    reg.register(Arc::new(
        MockComponent::new("first").with_stop_order(stop_order.clone()),
    ));
    // Middle component fails on stop
    reg.register(Arc::new(
        MockComponent::new("fail-middle").with_fail_on_stop("stop error"),
    ));
    reg.register(Arc::new(
        MockComponent::new("last").with_stop_order(stop_order.clone()),
    ));

    reg.start_all().await.unwrap();
    let result = reg.stop_all().await;
    assert!(result.is_err());

    let order = stop_order.lock();
    // Both non-failing components should have stopped (reverse order)
    assert_eq!(*order, vec!["last", "first"]);
}

// ── 7. health_all aggregation ────────────────────────────────────────────────

#[test]
fn health_all_empty_registry() {
    let reg = Registry::new();
    let healths = reg.health_all();
    assert!(healths.is_empty());
}

#[test]
fn health_all_mixed_statuses() {
    let mut reg = Registry::new();
    reg.register(Arc::new(MockComponent::new("healthy-comp")));
    reg.register(Arc::new(
        MockComponent::new("unhealthy-comp").with_unhealthy(),
    ));

    let healths = reg.health_all();
    assert_eq!(healths.len(), 2);
    assert_eq!(healths[0].status, HealthStatus::Healthy);
    assert_eq!(healths[1].status, HealthStatus::Unhealthy);
}

#[test]
fn health_all_preserves_component_names() {
    let mut reg = Registry::new();
    reg.register(Arc::new(MockComponent::new("db")));
    reg.register(Arc::new(MockComponent::new("cache")));
    reg.register(Arc::new(MockComponent::new("queue")));

    let healths = reg.health_all();
    let names: Vec<&str> = healths.iter().map(|h| h.name.as_str()).collect();
    assert_eq!(names, vec!["db", "cache", "queue"]);
}

// ── 8. LazyComponent initialization + health ─────────────────────────────────

#[tokio::test]
async fn lazy_component_defers_construction() {
    let constructed = Arc::new(AtomicBool::new(false));
    let constructed_clone = constructed.clone();

    let lazy = LazyComponent::new("lazy-db", move || {
        constructed_clone.store(true, Ordering::SeqCst);
        Arc::new(MockComponent::new("inner-db")) as Arc<dyn Component>
    });

    // Before start, factory should not have been called
    assert!(
        !constructed.load(Ordering::SeqCst),
        "factory should not be called before start"
    );

    // Health before start should be healthy (lazy default)
    assert!(lazy.health().is_healthy());

    // Start triggers construction
    lazy.start().await.unwrap();
    assert!(
        constructed.load(Ordering::SeqCst),
        "factory should be called after start"
    );
}

#[tokio::test]
async fn lazy_component_stop_before_start_is_noop() {
    let lazy = LazyComponent::new("lazy-noop", || {
        Arc::new(MockComponent::new("inner")) as Arc<dyn Component>
    });

    // Stop before start should succeed
    let result = lazy.stop().await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn lazy_component_health_delegates_after_start() {
    let lazy = LazyComponent::new("lazy-health", || {
        Arc::new(MockComponent::new("inner").with_unhealthy()) as Arc<dyn Component>
    });

    // Before start: healthy (lazy default)
    assert!(lazy.health().is_healthy());

    // After start: delegates to inner component
    lazy.start().await.unwrap();
    assert!(!lazy.health().is_healthy());
}

#[tokio::test]
async fn lazy_component_name_is_correct() {
    let lazy = LazyComponent::new("my-lazy", || {
        Arc::new(MockComponent::new("inner")) as Arc<dyn Component>
    });
    assert_eq!(lazy.name(), "my-lazy");
}

// ── 9. Registry config (concurrency, timeouts) ──────────────────────────────

#[test]
fn registry_config_defaults() {
    let config = RegistryConfig::default();
    assert_eq!(config.concurrency, 1);
    assert_eq!(config.start_timeout, Duration::from_secs(30));
    assert_eq!(config.stop_timeout, Duration::from_secs(30));
}

#[test]
fn registry_with_custom_config() {
    let config = RegistryConfig {
        concurrency: 4,
        start_timeout: Duration::from_secs(10),
        stop_timeout: Duration::from_secs(5),
    };
    let reg = Registry::with_config(config);
    assert!(reg.is_empty());
    assert_eq!(reg.len(), 0);
}

#[tokio::test]
async fn registry_concurrent_with_limited_concurrency() {
    let config = RegistryConfig {
        concurrency: 2,
        start_timeout: Duration::from_secs(5),
        stop_timeout: Duration::from_secs(5),
    };
    let mut reg = Registry::with_config(config);

    for i in 0..5 {
        reg.register(Arc::new(MockComponent::new(format!("comp-{}", i))));
    }

    let cancel = CancellationToken::new();
    let result = reg.start_all_concurrent(cancel).await;
    assert!(result.is_ok());
    assert_eq!(reg.len(), 5);
}

// ── 10. Graceful timeout handling ────────────────────────────────────────────

#[tokio::test]
async fn graceful_timeout_on_slow_stop_hook() {
    let cfg = TestCfg::default();
    let app = AppBuilder::new(cfg)
        .with_graceful_timeout(Duration::from_millis(100))
        .build()
        .unwrap()
        .before_stop(|_tok| async {
            tokio::time::sleep(Duration::from_secs(10)).await;
            Ok(())
        });

    // Use the shutdown_token to trigger shutdown via run() (which uses graceful_shutdown)
    let token = app.shutdown_token();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(50)).await;
        token.cancel();
    });

    let result = tokio::time::timeout(Duration::from_secs(5), app.run()).await;

    assert!(result.is_ok(), "should complete within 5 seconds");
    assert!(result.unwrap().is_err());
}

// ── 11. CancellationToken propagation ────────────────────────────────────────

#[tokio::test]
async fn shutdown_token_accessible() {
    let cfg = TestCfg::default();
    let app = AppBuilder::new(cfg).build().unwrap();
    let token = app.shutdown_token();
    assert!(!token.is_cancelled());
}

#[tokio::test]
async fn cancellation_token_propagated_to_task() {
    let cfg = TestCfg::default();
    let received_token = Arc::new(AtomicBool::new(false));
    let received_clone = received_token.clone();

    let app = AppBuilder::new(cfg).build().unwrap();

    let result = app
        .run_task(move |_cfg, cancel: CancellationToken| {
            let received = received_clone.clone();
            async move {
                received.store(!cancel.is_cancelled(), Ordering::SeqCst);
                Ok(())
            }
        })
        .await;

    assert!(result.is_ok());
    assert!(
        received_token.load(Ordering::SeqCst),
        "task should receive a valid (not cancelled) token"
    );
}

// ── 12. run_task completion triggers shutdown ─────────────────────────────────

#[tokio::test]
async fn run_task_completes_and_stops() {
    let stop_order: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let cfg = TestCfg::default();
    let comp = Arc::new(MockComponent::new("db").with_stop_order(stop_order.clone()));

    let app = AppBuilder::new(cfg).with_component(comp).build().unwrap();

    let result = app.run_task(|_cfg, _cancel| async { Ok(()) }).await;

    assert!(result.is_ok());
    let order = stop_order.lock();
    assert_eq!(*order, vec!["db"], "components should stop after task");
}

#[tokio::test]
async fn run_task_error_still_shuts_down() {
    let comp = Arc::new(MockComponent::new("db"));
    let comp_stop = comp.stop_count.clone();

    let cfg = TestCfg::default();
    let app = AppBuilder::new(cfg).with_component(comp).build().unwrap();

    let result = app
        .run_task(|_cfg, _cancel| async {
            Err::<(), AppError>(AppError::service_unavailable("task failed"))
        })
        .await;

    assert!(result.is_err());
    assert_eq!(
        comp_stop.load(Ordering::SeqCst),
        1,
        "component should be stopped even when task fails"
    );
}

#[tokio::test]
async fn run_task_receives_config() {
    let cfg = TestCfg::default();
    let app = AppBuilder::new(cfg).build().unwrap();

    let result = app
        .run_task(|cfg: Arc<TestCfg>, _cancel| async move {
            // Verify we can access config
            let _ = cfg.service_config();
            Ok(())
        })
        .await;

    assert!(result.is_ok());
}

// ── 13. Empty registry operations ────────────────────────────────────────────

#[tokio::test]
async fn empty_registry_start_all() {
    let reg = Registry::new();
    let result = reg.start_all().await;
    assert!(result.is_ok());
}

#[tokio::test]
async fn empty_registry_stop_all() {
    let reg = Registry::new();
    reg.stop_all().await.unwrap();
    // Should complete without panic
}

#[tokio::test]
async fn empty_registry_start_all_concurrent() {
    let reg = Registry::new();
    let cancel = CancellationToken::new();
    let result = reg.start_all_concurrent(cancel).await;
    assert!(result.is_ok());
}

#[test]
fn empty_registry_health_all() {
    let reg = Registry::new();
    assert!(reg.health_all().is_empty());
}

#[test]
fn empty_registry_len_and_is_empty() {
    let reg = Registry::new();
    assert_eq!(reg.len(), 0);
    assert!(reg.is_empty());

    let mut reg2 = Registry::new();
    reg2.register(Arc::new(MockComponent::new("x")));
    assert_eq!(reg2.len(), 1);
    assert!(!reg2.is_empty());
}

// ── Health type tests ────────────────────────────────────────────────────────

#[test]
fn health_healthy_constructor() {
    let h = Health::healthy("comp");
    assert_eq!(h.name, "comp");
    assert_eq!(h.status, HealthStatus::Healthy);
    assert!(h.message.is_none());
    assert!(h.is_healthy());
}

#[test]
fn health_degraded_constructor() {
    let h = Health::degraded("comp", "high latency");
    assert_eq!(h.status, HealthStatus::Degraded);
    assert_eq!(h.message, Some("high latency".to_string()));
    assert!(!h.is_healthy());
}

#[test]
fn health_unhealthy_constructor() {
    let h = Health::unhealthy("comp", "connection refused");
    assert_eq!(h.status, HealthStatus::Unhealthy);
    assert_eq!(h.message, Some("connection refused".to_string()));
    assert!(!h.is_healthy());
}

#[test]
fn health_status_display() {
    assert_eq!(format!("{}", HealthStatus::Healthy), "healthy");
    assert_eq!(format!("{}", HealthStatus::Degraded), "degraded");
    assert_eq!(format!("{}", HealthStatus::Unhealthy), "unhealthy");
}

#[test]
fn health_equality() {
    let h1 = Health::healthy("a");
    let h2 = Health::healthy("a");
    assert_eq!(h1, h2);

    let h3 = Health::healthy("b");
    assert_ne!(h1, h3);
}

// ── App with components full lifecycle ───────────────────────────────────────

#[tokio::test]
async fn full_lifecycle_with_components_and_hooks() {
    let order: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(Vec::new()));
    let cfg = TestCfg::default();

    let o_start = order.clone();
    let o_stop = order.clone();
    let comp = Arc::new(
        MockComponent::new("db")
            .with_start_order(order.clone())
            .with_stop_order(order.clone()),
    );

    let app = AppBuilder::new(cfg)
        .with_component(comp)
        .build()
        .unwrap()
        .before_start(move |_tok| {
            let o = o_start.clone();
            async move {
                o.lock().push("hook:start".into());
                Ok(())
            }
        })
        .before_stop(move |_tok| {
            let o = o_stop.clone();
            async move {
                o.lock().push("hook:stop".into());
                Ok(())
            }
        });

    let result = app.run_task(|_cfg, _cancel| async { Ok(()) }).await;
    assert!(result.is_ok());

    let executed = order.lock();
    assert_eq!(*executed, vec!["hook:start", "db", "hook:stop", "db"],);
}

#[tokio::test]
async fn component_start_failure_prevents_task() {
    let cfg = TestCfg::default();
    let comp = Arc::new(MockComponent::new("bad").with_fail_on_start("connection refused"));

    let task_ran = Arc::new(AtomicBool::new(false));
    let task_ran_clone = task_ran.clone();

    let app = AppBuilder::new(cfg).with_component(comp).build().unwrap();

    let result = app
        .run_task(move |_cfg, _cancel| {
            let ran = task_ran_clone.clone();
            async move {
                ran.store(true, Ordering::SeqCst);
                Ok(())
            }
        })
        .await;

    assert!(result.is_err());
    assert!(
        !task_ran.load(Ordering::SeqCst),
        "task should not run when component fails to start"
    );
}

#[tokio::test]
async fn app_config_accessible_from_task() {
    let cfg = TestCfg::default();
    let app = AppBuilder::new(cfg).build().unwrap();

    let result = app
        .run_task(|cfg: Arc<TestCfg>, _cancel| async move {
            let sc = cfg.service_config();
            assert_eq!(sc.name, "service"); // default name
            Ok(())
        })
        .await;

    assert!(result.is_ok());
}
