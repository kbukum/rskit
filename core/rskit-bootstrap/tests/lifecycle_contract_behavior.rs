#![allow(missing_docs)]

use std::sync::{
    Arc,
    atomic::{AtomicUsize, Ordering},
};
use std::time::Duration;

use async_trait::async_trait;
use rskit_bootstrap::{App, AppBuilder, Component, Health, LifecycleEventType};
use rskit_config::{AppConfig, ServiceConfig};
use rskit_errors::{AppError, AppResult, ErrorCode};

#[derive(Debug, Default, serde::Deserialize)]
struct TestConfig {
    #[serde(default)]
    service: ServiceConfig,
}

impl rskit_validation::Validate for TestConfig {
    fn validate(&self) -> Result<(), validator::ValidationErrors> {
        rskit_validation::Validate::validate(&self.service)
    }
}

impl AppConfig for TestConfig {
    fn apply_defaults(&mut self) {}

    fn service_config(&self) -> &ServiceConfig {
        &self.service
    }
}

struct CountingComponent {
    starts: Arc<AtomicUsize>,
    stops: Arc<AtomicUsize>,
    healthy: bool,
}

#[async_trait]
impl Component for CountingComponent {
    fn name(&self) -> &str {
        "counter"
    }

    async fn start(&self) -> AppResult<()> {
        self.starts.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    async fn stop(&self) -> AppResult<()> {
        self.stops.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }

    fn health(&self) -> Health {
        if self.healthy {
            Health::healthy(self.name())
        } else {
            Health::unhealthy(self.name(), "not ready")
        }
    }
}

#[tokio::test]
async fn app_builder_exposes_config_container_token_and_lifecycle_events() {
    let starts = Arc::new(AtomicUsize::new(0));
    let stops = Arc::new(AtomicUsize::new(0));
    let dependency = Arc::new(String::from("dependency"));
    let app = AppBuilder::new(TestConfig::default())
        .with_graceful_timeout(Duration::from_secs(1))
        .with_dependency(Arc::clone(&dependency))
        .with_component(Arc::new(CountingComponent {
            starts: Arc::clone(&starts),
            stops: Arc::clone(&stops),
            healthy: true,
        }))
        .build()
        .unwrap();

    assert!(app.container().resolve::<String>().is_ok());
    assert_eq!(app.config().service_config().name, "service");
    let mut events = app.lifecycle_event_bus().subscribe();
    let token = app.shutdown_token();
    assert!(!token.is_cancelled());

    let started = app.start().await.unwrap();
    assert_eq!(starts.load(Ordering::SeqCst), 1);
    assert!(started.container().resolve::<String>().is_ok());
    assert_eq!(started.config().service_config().name, "service");
    assert!(!started.shutdown_token().is_cancelled());
    let stopped = started.stop().await.unwrap();
    assert_eq!(stops.load(Ordering::SeqCst), 1);
    assert_eq!(stopped.config().service_config().name, "service");
    let _ = stopped.lifecycle_event_bus();

    assert_eq!(
        events.recv().await.unwrap().kind(),
        LifecycleEventType::BeforeStart
    );
    assert_eq!(
        events.recv().await.unwrap().kind(),
        LifecycleEventType::AfterStart
    );
    assert_eq!(
        events.recv().await.unwrap().kind(),
        LifecycleEventType::BeforeStop
    );
    assert_eq!(
        events.recv().await.unwrap().kind(),
        LifecycleEventType::AfterStop
    );
}

#[tokio::test]
async fn custom_container_and_lifecycle_bus_are_used_and_after_start_failure_rolls_back() {
    let container = Arc::new(rskit_di::Container::new());
    container.register(Arc::new(42_u32));
    let bus = rskit_hook::EventBus::new(rskit_hook::EventBusConfig { capacity: 8 });
    let mut events = bus.subscribe();
    let stops = Arc::new(AtomicUsize::new(0));

    let app = AppBuilder::new(TestConfig::default())
        .with_container(Arc::clone(&container))
        .with_lifecycle_event_bus(bus)
        .with_component(Arc::new(CountingComponent {
            starts: Arc::new(AtomicUsize::new(0)),
            stops: Arc::clone(&stops),
            healthy: true,
        }))
        .after_start(|_| async {
            Err::<(), _>(AppError::new(ErrorCode::Internal, "after start failed"))
        })
        .build()
        .unwrap();

    assert_eq!(*app.container().resolve::<u32>().unwrap(), 42);
    let error = match app.start().await {
        Ok(_) => panic!("after_start failure should fail startup"),
        Err(error) => error,
    };
    assert!(error.to_string().contains("after_start hooks failed"));
    assert_eq!(stops.load(Ordering::SeqCst), 1);
    assert_eq!(
        events.recv().await.unwrap().kind(),
        LifecycleEventType::BeforeStart
    );
    assert_eq!(
        events.recv().await.unwrap().kind(),
        LifecycleEventType::AfterStart
    );
    assert_eq!(
        events.recv().await.unwrap().kind(),
        LifecycleEventType::BeforeStop
    );
}

#[tokio::test]
async fn run_task_returns_task_error_and_still_stops_components() {
    let stops = Arc::new(AtomicUsize::new(0));
    let app = App::<rskit_bootstrap::Built, TestConfig>::builder(TestConfig::default())
        .with_component(Arc::new(CountingComponent {
            starts: Arc::new(AtomicUsize::new(0)),
            stops: Arc::clone(&stops),
            healthy: true,
        }))
        .build()
        .unwrap();

    let error = app
        .run_task(|_, _| async { Err::<(), _>(AppError::new(ErrorCode::Internal, "task failed")) })
        .await
        .unwrap_err();
    assert!(error.to_string().contains("task failed"));
    assert_eq!(stops.load(Ordering::SeqCst), 1);
}

#[tokio::test]
async fn unhealthy_component_rolls_back_startup_and_runs_stop_hooks() {
    let stops = Arc::new(AtomicUsize::new(0));
    let before_stop = Arc::new(AtomicUsize::new(0));
    let hook_count = Arc::clone(&before_stop);
    let app = AppBuilder::new(TestConfig::default())
        .with_component(Arc::new(CountingComponent {
            starts: Arc::new(AtomicUsize::new(0)),
            stops: Arc::clone(&stops),
            healthy: false,
        }))
        .before_stop(move |_| {
            let hook_count = Arc::clone(&hook_count);
            async move {
                hook_count.fetch_add(1, Ordering::SeqCst);
                Ok(())
            }
        })
        .build()
        .unwrap();

    let error = match app.start().await {
        Ok(_) => panic!("unhealthy component should fail startup"),
        Err(error) => error,
    };
    assert_eq!(error.code(), ErrorCode::ServiceUnavailable);
    assert_eq!(stops.load(Ordering::SeqCst), 1);
    assert_eq!(before_stop.load(Ordering::SeqCst), 1);
}

#[test]
fn lifecycle_event_metadata_is_stable() {
    assert_eq!(
        LifecycleEventType::BeforeStart.as_str(),
        "bootstrap:before_start"
    );
    assert_eq!(
        LifecycleEventType::AfterStart.as_str(),
        "bootstrap:after_start"
    );
    assert_eq!(
        LifecycleEventType::BeforeStop.as_str(),
        "bootstrap:before_stop"
    );
    assert_eq!(
        LifecycleEventType::AfterStop.as_str(),
        "bootstrap:after_stop"
    );
}
