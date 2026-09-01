use std::marker::PhantomData;
use std::sync::Arc;
use std::time::Duration;

use rskit_component::{Component, Registry};
use rskit_config::AppConfig;
use rskit_di::Container;
use rskit_errors::{AppError, AppResult, ErrorCode};
use rskit_hook::{EventBus, EventBusConfig};
use rskit_provider::Provider;
use tokio_util::sync::CancellationToken;

use crate::hooks::LifecycleEvent;
use crate::lifecycle::{
    AsyncHook, LifecycleHooks, LifecyclePhase, make_hook, record_stop_error, run_hooks,
    wait_for_signal,
};

/// Typestate marker retained only as the unbuildable initial marker.
///
/// Applications are constructed through [`AppBuilder`], which validates config
/// and returns [`App<Built, C>`].
pub struct Unconfigured;

/// Typestate marker for a built application whose components are not running.
pub struct Built;

/// Typestate marker for an application with started components.
pub struct Started;

/// Typestate marker for an application that has completed shutdown.
pub struct Stopped;

/// Builder for [`App`].
///
/// The builder is the composition root for configuration validation, component registration,
/// dependency injection, provider registration, and lifecycle hook ordering.
pub struct AppBuilder<C: AppConfig> {
    config: C,
    graceful_timeout: Duration,
    components: Vec<Arc<dyn Component>>,
    container: Arc<Container>,
    lifecycle_events: EventBus<LifecycleEvent>,
    hooks: LifecycleHooks,
}

impl<C: AppConfig> AppBuilder<C> {
    /// Create a new builder with the given application configuration.
    #[must_use]
    pub fn new(config: C) -> Self {
        Self {
            config,
            graceful_timeout: Duration::from_secs(30),
            components: Vec::new(),
            container: Arc::new(Container::new()),
            lifecycle_events: EventBus::new(EventBusConfig::default()),
            hooks: LifecycleHooks::default(),
        }
    }

    /// Set the graceful shutdown timeout.
    #[must_use]
    pub fn with_graceful_timeout(mut self, timeout: Duration) -> Self {
        self.graceful_timeout = timeout;
        self
    }

    /// Register a component to be started and stopped by the app lifecycle.
    #[must_use]
    pub fn with_component(mut self, component: Arc<dyn Component>) -> Self {
        self.components.push(component);
        self
    }

    /// Use an existing typed DI container as the application container.
    #[must_use]
    pub fn with_container(mut self, container: Arc<Container>) -> Self {
        self.container = container;
        self
    }

    /// Use an existing bounded lifecycle event bus.
    #[must_use]
    pub fn with_lifecycle_event_bus(mut self, lifecycle_events: EventBus<LifecycleEvent>) -> Self {
        self.lifecycle_events = lifecycle_events;
        self
    }

    /// Register a typed dependency in the application DI container.
    #[must_use]
    pub fn with_dependency<T>(self, dependency: Arc<T>) -> Self
    where
        T: Send + Sync + 'static,
    {
        self.container.register(dependency);
        self
    }

    /// Register a provider implementation in the application DI container.
    #[must_use]
    pub fn with_provider<P>(self, provider: Arc<P>) -> Self
    where
        P: Provider + Send + Sync + 'static,
    {
        self.container.register(provider);
        self
    }

    /// Register a hook called once during startup before any component or start hook runs.
    #[must_use]
    pub fn configure<F, Fut>(mut self, hook: F) -> Self
    where
        F: Fn(CancellationToken) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = AppResult<()>> + Send + 'static,
    {
        self.hooks.configure.push(make_hook(hook));
        self
    }

    /// Register a hook called before components start.
    #[must_use]
    pub fn before_start<F, Fut>(mut self, hook: F) -> Self
    where
        F: Fn(CancellationToken) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = AppResult<()>> + Send + 'static,
    {
        self.hooks.before_start.push(make_hook(hook));
        self
    }

    /// Register a hook called after components start and readiness checks pass.
    #[must_use]
    pub fn after_start<F, Fut>(mut self, hook: F) -> Self
    where
        F: Fn(CancellationToken) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = AppResult<()>> + Send + 'static,
    {
        self.hooks.after_start.push(make_hook(hook));
        self
    }

    /// Register a hook called once during startup after the application is fully started and ready.
    #[must_use]
    pub fn ready<F, Fut>(mut self, hook: F) -> Self
    where
        F: Fn(CancellationToken) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = AppResult<()>> + Send + 'static,
    {
        self.hooks.ready.push(make_hook(hook));
        self
    }

    /// Register a hook called before components stop.
    #[must_use]
    pub fn before_stop<F, Fut>(mut self, hook: F) -> Self
    where
        F: Fn(CancellationToken) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = AppResult<()>> + Send + 'static,
    {
        self.hooks.before_stop.push(make_hook(hook));
        self
    }

    /// Register a hook called after components stop.
    #[must_use]
    pub fn after_stop<F, Fut>(mut self, hook: F) -> Self
    where
        F: Fn(CancellationToken) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = AppResult<()>> + Send + 'static,
    {
        self.hooks.after_stop.push(make_hook(hook));
        self
    }

    /// Validate config and build the application in the [`Built`] state.
    pub fn build(self) -> AppResult<App<Built, C>> {
        self.config
            .validate()
            .map_err(|error| AppError::invalid_input("config", error.to_string()))?;

        let mut registry = Registry::new();
        for component in self.components {
            registry.register(component);
        }

        Ok(App {
            _state: PhantomData,
            config: Arc::new(self.config),
            container: self.container,
            registry,
            lifecycle_events: self.lifecycle_events,
            hooks: self.hooks,
            graceful_timeout: self.graceful_timeout,
            shutdown_token: CancellationToken::new(),
        })
    }
}

/// Application orchestrator with typestate lifecycle.
///
/// Valid transitions are compile-time scoped:
///
/// ```text
/// AppBuilder::build() -> App<Built, C>
/// App<Built, C>::start() -> App<Started, C>
/// App<Started, C>::stop() -> App<Stopped, C>
/// ```
pub struct App<S, C> {
    _state: PhantomData<S>,
    config: Arc<C>,
    container: Arc<Container>,
    registry: Registry,
    lifecycle_events: EventBus<LifecycleEvent>,
    hooks: LifecycleHooks,
    graceful_timeout: Duration,
    shutdown_token: CancellationToken,
}

impl<C: AppConfig> App<Built, C> {
    /// Create an [`AppBuilder`].
    pub fn builder(config: C) -> AppBuilder<C> {
        AppBuilder::new(config)
    }
}

impl<C> App<Built, C> {
    /// Return a clone of the shared application configuration.
    #[must_use]
    pub fn config(&self) -> Arc<C> {
        Arc::clone(&self.config)
    }

    /// Return the typed application DI container.
    #[must_use]
    pub fn container(&self) -> Arc<Container> {
        Arc::clone(&self.container)
    }

    /// Return the bounded lifecycle event bus used by this app.
    #[must_use]
    pub fn lifecycle_event_bus(&self) -> EventBus<LifecycleEvent> {
        self.lifecycle_events.clone()
    }

    /// Clone the shutdown token to share with long-running tasks.
    #[must_use]
    pub fn shutdown_token(&self) -> CancellationToken {
        self.shutdown_token.clone()
    }

    /// Register a hook called once during startup before any component or start hook runs.
    #[must_use]
    pub fn configure<F, Fut>(mut self, hook: F) -> Self
    where
        F: Fn(CancellationToken) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = AppResult<()>> + Send + 'static,
    {
        self.hooks.configure.push(make_hook(hook));
        self
    }

    /// Register a hook called before components start.
    #[must_use]
    pub fn before_start<F, Fut>(mut self, hook: F) -> Self
    where
        F: Fn(CancellationToken) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = AppResult<()>> + Send + 'static,
    {
        self.hooks.before_start.push(make_hook(hook));
        self
    }

    /// Register a hook called after components start and readiness checks pass.
    #[must_use]
    pub fn after_start<F, Fut>(mut self, hook: F) -> Self
    where
        F: Fn(CancellationToken) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = AppResult<()>> + Send + 'static,
    {
        self.hooks.after_start.push(make_hook(hook));
        self
    }

    /// Register a hook called once during startup after the application is fully started and ready.
    #[must_use]
    pub fn ready<F, Fut>(mut self, hook: F) -> Self
    where
        F: Fn(CancellationToken) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = AppResult<()>> + Send + 'static,
    {
        self.hooks.ready.push(make_hook(hook));
        self
    }

    /// Register a hook called before components stop.
    #[must_use]
    pub fn before_stop<F, Fut>(mut self, hook: F) -> Self
    where
        F: Fn(CancellationToken) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = AppResult<()>> + Send + 'static,
    {
        self.hooks.before_stop.push(make_hook(hook));
        self
    }

    /// Register a hook called after components stop.
    #[must_use]
    pub fn after_stop<F, Fut>(mut self, hook: F) -> Self
    where
        F: Fn(CancellationToken) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = AppResult<()>> + Send + 'static,
    {
        self.hooks.after_stop.push(make_hook(hook));
        self
    }
}

impl<C: AppConfig> App<Built, C> {
    /// Start components and transition to [`Started`].
    pub async fn start(self) -> AppResult<App<Started, C>> {
        crate::summary::print_startup(self.config.service_config(), self.registry.len());
        self.run_hooks(LifecyclePhase::Configure, &self.hooks.configure)
            .await?;
        self.run_hooks(LifecyclePhase::BeforeStart, &self.hooks.before_start)
            .await?;

        if let Err(error) = self.registry.start_all().await {
            return Err(self
                .rollback_startup_failure(error.context("component startup failed"), false)
                .await);
        }

        if let Err(error) = self.ready_check() {
            return Err(self
                .rollback_startup_failure(error.context("component readiness failed"), true)
                .await);
        }

        if let Err(error) = self
            .run_hooks(LifecyclePhase::AfterStart, &self.hooks.after_start)
            .await
        {
            return Err(self
                .rollback_startup_failure(error.context("after_start hooks failed"), true)
                .await);
        }

        if let Err(error) = self
            .run_hooks(LifecyclePhase::Ready, &self.hooks.ready)
            .await
        {
            return Err(self
                .rollback_startup_failure(error.context("ready hooks failed"), true)
                .await);
        }

        Ok(App {
            _state: PhantomData,
            config: self.config,
            container: self.container,
            registry: self.registry,
            lifecycle_events: self.lifecycle_events,
            hooks: self.hooks,
            graceful_timeout: self.graceful_timeout,
            shutdown_token: self.shutdown_token,
        })
    }

    /// Run as a long-running service until a signal or shutdown token fires.
    pub async fn run(self) -> AppResult<()> {
        let started = self.start().await?;
        wait_for_signal(started.shutdown_token.clone()).await;
        started.stop().await.map(|_| ())
    }

    /// Run a finite task after startup, then stop the app.
    pub async fn run_task<F, Fut>(self, task: F) -> AppResult<()>
    where
        F: FnOnce(Arc<C>, CancellationToken) -> Fut,
        Fut: std::future::Future<Output = AppResult<()>>,
    {
        let started = self.start().await?;
        let task_result = tokio::select! {
            result = task(Arc::clone(&started.config), started.shutdown_token.clone()) => result,
            _ = wait_for_signal_owned(started.shutdown_token.clone()) => {
                Ok(())
            }
        };
        let stop_result = started.stop().await.map(|_| ());
        match (task_result, stop_result) {
            (Ok(()), Ok(())) => Ok(()),
            (Ok(()), Err(stop_error)) => Err(stop_error),
            (Err(task_error), Ok(())) => Err(task_error),
            (Err(task_error), Err(stop_error)) => Err(task_error
                .context("application shutdown also failed after task error")
                .with_detail("shutdown_error", stop_error.to_string())),
        }
    }

    fn ready_check(&self) -> AppResult<()> {
        let unhealthy = self
            .registry
            .health_all()
            .into_iter()
            .filter(|health| !health.is_healthy())
            .map(|health| match health.message {
                Some(message) => format!("{}={}({message})", health.name, health.status),
                None => format!("{}={}", health.name, health.status),
            })
            .collect::<Vec<_>>();

        if unhealthy.is_empty() {
            Ok(())
        } else {
            Err(AppError::service_unavailable(format!(
                "unhealthy components: {}",
                unhealthy.join(", ")
            )))
        }
    }

    async fn rollback_startup_failure(
        &self,
        mut error: AppError,
        stop_components: bool,
    ) -> AppError {
        if let Err(stop_hook_error) = self
            .run_hooks(LifecyclePhase::BeforeStop, &self.hooks.before_stop)
            .await
        {
            tracing::warn!(error = %stop_hook_error, "before_stop hook failed during startup rollback");
            error = error.context(format!(
                "startup rollback before_stop hooks failed: {stop_hook_error}"
            ));
        }

        if stop_components && let Err(stop_error) = self.registry.stop_all().await {
            tracing::warn!(error = %stop_error, "component rollback error");
            error = error.context(format!(
                "startup rollback component stop failed: {stop_error}"
            ));
        }
        error
    }
}

impl<C> App<Built, C> {
    async fn run_hooks(&self, phase: LifecyclePhase, hooks: &[AsyncHook]) -> AppResult<()> {
        run_hooks(
            phase,
            hooks,
            self.shutdown_token.clone(),
            &self.lifecycle_events,
        )
        .await
    }
}

impl<C> App<Started, C> {
    /// Return a clone of the shared application configuration.
    #[must_use]
    pub fn config(&self) -> Arc<C> {
        Arc::clone(&self.config)
    }

    /// Return the typed application DI container.
    #[must_use]
    pub fn container(&self) -> Arc<Container> {
        Arc::clone(&self.container)
    }

    /// Return the bounded lifecycle event bus used by this app.
    #[must_use]
    pub fn lifecycle_event_bus(&self) -> EventBus<LifecycleEvent> {
        self.lifecycle_events.clone()
    }

    /// Clone the shutdown token to share with long-running tasks.
    #[must_use]
    pub fn shutdown_token(&self) -> CancellationToken {
        self.shutdown_token.clone()
    }

    /// Stop components and transition to [`Stopped`].
    pub async fn stop(self) -> AppResult<App<Stopped, C>> {
        let mut stop_error: Option<AppError> = None;
        let stop_result = tokio::time::timeout(
            self.graceful_timeout,
            run_hooks(
                LifecyclePhase::BeforeStop,
                &self.hooks.before_stop,
                self.shutdown_token.clone(),
                &self.lifecycle_events,
            ),
        )
        .await;

        match stop_result {
            Ok(Ok(())) => {}
            Ok(Err(error)) => {
                tracing::warn!(error = %error, "before_stop hook error");
                stop_error = Some(error);
            }
            Err(_) => {
                tracing::warn!("graceful shutdown timeout elapsed during before_stop hooks");
                stop_error = Some(AppError::new(
                    ErrorCode::Timeout,
                    "graceful shutdown timeout elapsed during before_stop hooks",
                ));
            }
        }

        if let Err(error) = self.registry.stop_all().await {
            tracing::warn!(error = %error, "component shutdown error");
            record_stop_error(&mut stop_error, error, "component shutdown failed");
        }

        if let Err(error) = run_hooks(
            LifecyclePhase::AfterStop,
            &self.hooks.after_stop,
            self.shutdown_token.clone(),
            &self.lifecycle_events,
        )
        .await
        {
            record_stop_error(&mut stop_error, error, "after_stop hooks failed");
        }

        if let Err(error) = self.container.close().await {
            record_stop_error(&mut stop_error, error, "container close failed");
        }
        tracing::info!("shutdown complete");

        if let Some(error) = stop_error {
            return Err(error);
        }

        Ok(App {
            _state: PhantomData,
            config: self.config,
            container: self.container,
            registry: self.registry,
            lifecycle_events: self.lifecycle_events,
            hooks: self.hooks,
            graceful_timeout: self.graceful_timeout,
            shutdown_token: self.shutdown_token,
        })
    }
}

impl<C> App<Stopped, C> {
    /// Return a clone of the shared application configuration.
    #[must_use]
    pub fn config(&self) -> Arc<C> {
        Arc::clone(&self.config)
    }

    /// Return the bounded lifecycle event bus used by this app.
    #[must_use]
    pub fn lifecycle_event_bus(&self) -> EventBus<LifecycleEvent> {
        self.lifecycle_events.clone()
    }
}

async fn wait_for_signal_owned(token: CancellationToken) {
    wait_for_signal(token).await;
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use parking_lot::Mutex;
    use rskit_config::{AppConfig as _, ServiceConfig};
    use rskit_provider::Provider;
    use tokio_util::sync::CancellationToken;

    use super::AppBuilder;

    #[derive(Debug, Default, serde::Deserialize)]
    struct TestCfg {
        #[serde(default)]
        service: rskit_config::ServiceConfig,
    }

    impl rskit_validation::Validate for TestCfg {
        fn validate(&self) -> Result<(), validator::ValidationErrors> {
            rskit_validation::Validate::validate(&self.service)
        }
    }

    impl rskit_config::AppConfig for TestCfg {
        fn apply_defaults(&mut self) {}

        fn service_config(&self) -> &ServiceConfig {
            &self.service
        }
    }

    struct TestProvider;

    impl Provider for TestProvider {
        fn name(&self) -> &'static str {
            "test-provider"
        }
    }

    #[tokio::test]
    async fn app_builder_builds_successfully() {
        let result = AppBuilder::new(TestCfg::default()).build();
        assert!(result.is_ok());
    }

    #[tokio::test]
    async fn lifecycle_hooks_run_in_order() {
        let order = Arc::new(Mutex::new(Vec::new()));
        let configure = Arc::clone(&order);
        let before_start = Arc::clone(&order);
        let after_start = Arc::clone(&order);
        let ready = Arc::clone(&order);
        let before_stop = Arc::clone(&order);
        let after_stop = Arc::clone(&order);

        let app = AppBuilder::new(TestCfg::default())
            .configure(move |_token| {
                let order = Arc::clone(&configure);
                async move {
                    order.lock().push("configure");
                    Ok(())
                }
            })
            .before_start(move |_token| {
                let order = Arc::clone(&before_start);
                async move {
                    order.lock().push("before_start");
                    Ok(())
                }
            })
            .after_start(move |_token| {
                let order = Arc::clone(&after_start);
                async move {
                    order.lock().push("after_start");
                    Ok(())
                }
            })
            .ready(move |_token| {
                let order = Arc::clone(&ready);
                async move {
                    order.lock().push("ready");
                    Ok(())
                }
            })
            .before_stop(move |_token| {
                let order = Arc::clone(&before_stop);
                async move {
                    order.lock().push("before_stop");
                    Ok(())
                }
            })
            .after_stop(move |_token| {
                let order = Arc::clone(&after_stop);
                async move {
                    order.lock().push("after_stop");
                    Ok(())
                }
            })
            .build()
            .expect("build should succeed");

        app.run_task(|_cfg: Arc<TestCfg>, _cancel: CancellationToken| async move { Ok(()) })
            .await
            .expect("run_task should complete");

        assert_eq!(
            *order.lock(),
            vec![
                "configure",
                "before_start",
                "after_start",
                "ready",
                "before_stop",
                "after_stop"
            ]
        );
    }

    #[tokio::test]
    async fn lifecycle_events_are_published() {
        let app = AppBuilder::new(TestCfg::default())
            .build()
            .expect("build should succeed");
        let mut subscriber = app.lifecycle_event_bus().subscribe();

        app.run_task(|_cfg: Arc<TestCfg>, _cancel: CancellationToken| async move { Ok(()) })
            .await
            .expect("run_task should complete");

        let configure = subscriber.recv().await.expect("configure event");
        let before_start = subscriber.recv().await.expect("before_start event");
        let after_start = subscriber.recv().await.expect("after_start event");
        let ready = subscriber.recv().await.expect("ready event");
        let before_stop = subscriber.recv().await.expect("before_stop event");
        let after_stop = subscriber.recv().await.expect("after_stop event");
        assert_eq!(configure.kind(), crate::LifecycleEventType::Configure);
        assert_eq!(before_start.kind(), crate::LifecycleEventType::BeforeStart);
        assert_eq!(after_start.kind(), crate::LifecycleEventType::AfterStart);
        assert_eq!(ready.kind(), crate::LifecycleEventType::Ready);
        assert_eq!(before_stop.kind(), crate::LifecycleEventType::BeforeStop);
        assert_eq!(after_stop.kind(), crate::LifecycleEventType::AfterStop);
    }

    #[tokio::test]
    async fn builder_registers_provider_dependency_and_started_stopped_accessors() {
        let dependency = Arc::new(7_usize);
        let provider = Arc::new(TestProvider);
        let app = AppBuilder::new(TestCfg::default())
            .with_dependency(Arc::clone(&dependency))
            .with_provider(Arc::clone(&provider))
            .after_stop(|_| async { Ok(()) })
            .build()
            .expect("build should succeed");

        assert_eq!(*app.container().resolve::<usize>().unwrap(), 7);
        assert_eq!(
            app.container().resolve::<TestProvider>().unwrap().name(),
            "test-provider"
        );
        let _built_bus = app.lifecycle_event_bus();

        let started = app.start().await.expect("start should succeed");
        assert_eq!(started.config().service_config().name, "service");
        assert_eq!(*started.container().resolve::<usize>().unwrap(), 7);
        let _started_bus = started.lifecycle_event_bus();
        let _shutdown = started.shutdown_token();

        let stopped = started.stop().await.expect("stop should succeed");
        assert_eq!(stopped.config().service_config().name, "service");
        let _stopped_bus = stopped.lifecycle_event_bus();
    }

    #[tokio::test]
    async fn run_task_exits_when_shutdown_token_is_cancelled() {
        let app = AppBuilder::new(TestCfg::default())
            .build()
            .expect("build should succeed");

        app.run_task(|_cfg: Arc<TestCfg>, cancel: CancellationToken| async move {
            cancel.cancel();
            std::future::pending::<rskit_errors::AppResult<()>>().await
        })
        .await
        .expect("shutdown cancellation should stop run_task");
    }

    #[tokio::test]
    async fn app_run_task_executes_and_exits() {
        let app = AppBuilder::new(TestCfg::default())
            .build()
            .expect("build should succeed");
        let runs = Arc::new(AtomicUsize::new(0));
        let run_counter = Arc::clone(&runs);

        app.run_task(move |_cfg: Arc<TestCfg>, _cancel: CancellationToken| {
            let runs = Arc::clone(&run_counter);
            async move {
                runs.fetch_add(1, Ordering::SeqCst);
                Ok::<(), rskit_errors::AppError>(())
            }
        })
        .await
        .expect("task should run");

        assert_eq!(runs.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn hook_failure_stops_startup() {
        let app = AppBuilder::new(TestCfg::default())
            .before_start(|_| async {
                Err::<(), _>(rskit_errors::AppError::new(
                    rskit_errors::ErrorCode::Internal,
                    "hard fail",
                ))
            })
            .build()
            .expect("build should succeed");

        let error = app
            .run_task(|_cfg: Arc<TestCfg>, _cancel: CancellationToken| async move { Ok(()) })
            .await
            .expect_err("hook failure should fail startup");
        assert!(error.to_string().contains("before_start hook failed"));
    }

    #[tokio::test]
    async fn stop_hooks_run_after_shutdown_token_is_cancelled() {
        let ran = Arc::new(AtomicUsize::new(0));
        let before_stop_ran = Arc::clone(&ran);
        let after_stop_ran = Arc::clone(&ran);

        let app = AppBuilder::new(TestCfg::default())
            .before_stop(move |_token| {
                let ran = Arc::clone(&before_stop_ran);
                async move {
                    ran.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                }
            })
            .after_stop(move |_token| {
                let ran = Arc::clone(&after_stop_ran);
                async move {
                    ran.fetch_add(1, Ordering::SeqCst);
                    Ok(())
                }
            })
            .build()
            .expect("build should succeed")
            .start()
            .await
            .expect("start should succeed");

        app.shutdown_token().cancel();
        app.stop().await.expect("stop hooks should still run");

        assert_eq!(ran.load(Ordering::SeqCst), 2);
    }
}
