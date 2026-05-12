use std::marker::PhantomData;
use std::sync::{Arc, mpsc::sync_channel};
use std::time::Duration;

use futures_util::future::BoxFuture;
use rskit_component::{Component, Registry};
use rskit_config::AppConfig;
use rskit_errors::{AppError, AppResult};
use rskit_hook::{HookError, HookResult, Registry as HookRegistry};
use tokio_util::sync::CancellationToken;

use crate::hooks::{AppReady, AppStarted, AppStopping, LifecyclePhase};

/// Typestate marker: App not yet started.
pub struct Unconfigured;

type AsyncHook =
    Arc<dyn Fn(CancellationToken) -> BoxFuture<'static, AppResult<()>> + Send + Sync + 'static>;

fn make_hook<F, Fut>(f: F) -> AsyncHook
where
    F: Fn(CancellationToken) -> Fut + Send + Sync + 'static,
    Fut: std::future::Future<Output = AppResult<()>> + Send + 'static,
{
    Arc::new(move |token| Box::pin(f(token)))
}

fn register_lifecycle_hook(hooks: &Arc<HookRegistry>, phase: LifecyclePhase, hook: AsyncHook) {
    let handler = move |cancel: CancellationToken, handle: &tokio::runtime::Handle| {
        let hook = Arc::clone(&hook);
        let (sender, receiver) = sync_channel(1);
        handle.spawn(async move {
            let result = hook(cancel).await;
            let _ = sender.send(result);
        });

        match receiver.recv() {
            Ok(Ok(())) => Ok(()),
            Ok(Err(error)) => {
                tracing::error!(error = ?error, "lifecycle hook failed");
                Err(HookError::fatal(error.to_string()))
            }
            Err(error) => Err(HookError::fatal(AppError::internal(error).to_string())),
        }
    };

    match phase {
        LifecyclePhase::Start => {
            let _unsubscribe = hooks.on::<AppStarted>(0, move |cancel, event| {
                let event = event.as_any().downcast_ref::<AppStarted>().unwrap();
                handler(cancel, &event.runtime_handle)
            });
        }
        LifecyclePhase::Ready => {
            let _unsubscribe = hooks.on::<AppReady>(0, move |cancel, event| {
                let event = event.as_any().downcast_ref::<AppReady>().unwrap();
                handler(cancel, &event.runtime_handle)
            });
        }
        LifecyclePhase::Stop => {
            let _unsubscribe = hooks.on::<AppStopping>(0, move |cancel, event| {
                let event = event.as_any().downcast_ref::<AppStopping>().unwrap();
                handler(cancel, &event.runtime_handle)
            });
        }
    }
}

fn phase_label(phase: LifecyclePhase) -> &'static str {
    match phase {
        LifecyclePhase::Start => "on_start",
        LifecyclePhase::Ready => "on_ready",
        LifecyclePhase::Stop => "on_stop",
    }
}

fn hook_result_to_error(phase: LifecyclePhase, result: HookResult) -> AppResult<()> {
    match result {
        Ok(()) => Ok(()),
        Err(error) => {
            if error.is_fatal() {
                let phase = phase_label(phase);
                tracing::error!(phase, ?error, "fatal hook error");
                return Err(AppError::internal(error).context(format!("{phase} hook failed")));
            }

            tracing::warn!(
                phase = phase_label(phase),
                error = %error,
                "non-fatal hook error"
            );
            Ok(())
        }
    }
}

/// Builder for [`App`]. Validates config before handing off to the lifecycle.
pub struct AppBuilder<C: AppConfig> {
    config: C,
    graceful_timeout: Duration,
    components: Vec<Arc<dyn Component>>,
    hooks: Arc<HookRegistry>,
}

impl<C: AppConfig> AppBuilder<C> {
    /// Create a new builder with the given application configuration.
    pub fn new(config: C) -> Self {
        Self {
            config,
            graceful_timeout: Duration::from_secs(30),
            components: Vec::new(),
            hooks: Arc::new(HookRegistry::new()),
        }
    }

    /// Set the graceful shutdown timeout (default: 30 s).
    #[must_use]
    pub fn with_graceful_timeout(mut self, timeout: Duration) -> Self {
        self.graceful_timeout = timeout;
        self
    }

    /// Register a component to be started and stopped by the app.
    #[must_use]
    pub fn with_component(mut self, component: Arc<dyn Component>) -> Self {
        self.components.push(component);
        self
    }

    /// Register a hook called after components start and before readiness checks.
    #[must_use]
    pub fn on_start<F, Fut>(self, hook: F) -> Self
    where
        F: Fn(CancellationToken) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = AppResult<()>> + Send + 'static,
    {
        let hook = make_hook(hook);
        register_lifecycle_hook(&self.hooks, LifecyclePhase::Start, hook);
        self
    }

    /// Register a hook called after readiness checks and before the app is ready.
    #[must_use]
    pub fn on_ready<F, Fut>(self, hook: F) -> Self
    where
        F: Fn(CancellationToken) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = AppResult<()>> + Send + 'static,
    {
        let hook = make_hook(hook);
        register_lifecycle_hook(&self.hooks, LifecyclePhase::Ready, hook);
        self
    }

    /// Register a hook called during graceful shutdown before components stop.
    #[must_use]
    pub fn on_stop<F, Fut>(self, hook: F) -> Self
    where
        F: Fn(CancellationToken) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = AppResult<()>> + Send + 'static,
    {
        let hook = make_hook(hook);
        register_lifecycle_hook(&self.hooks, LifecyclePhase::Stop, hook);
        self
    }

    /// Validate config and build the App.
    pub fn build(self) -> AppResult<App<Unconfigured, C>> {
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
            registry,
            on_configure: Vec::new(),
            hooks: self.hooks,
            graceful_timeout: self.graceful_timeout,
            shutdown_token: CancellationToken::new(),
        })
    }
}

/// Application orchestrator with typestate lifecycle.
///
/// # Lifecycle
///
/// ```text
/// build()
///   → on_configure hooks
///   → start_all components
///   → on_start hooks
///   → ready_check
///   → on_ready hooks
///   → wait for SIGINT/SIGTERM or manual cancel
///   → on_stop hooks
///   → stop_all components (LIFO)
/// ```
pub struct App<S, C> {
    _state: PhantomData<S>,
    config: Arc<C>,
    registry: Registry,
    on_configure: Vec<AsyncHook>,
    hooks: Arc<HookRegistry>,
    graceful_timeout: Duration,
    shutdown_token: CancellationToken,
}

impl<C: AppConfig> App<Unconfigured, C> {
    /// Convenience: creates an [`AppBuilder`].
    pub fn builder(config: C) -> AppBuilder<C> {
        AppBuilder::new(config)
    }

    /// Register a hook called before components are started.
    #[must_use]
    pub fn on_configure<F, Fut>(mut self, hook: F) -> Self
    where
        F: Fn(CancellationToken) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = AppResult<()>> + Send + 'static,
    {
        self.on_configure.push(make_hook(hook));
        self
    }

    /// Register a hook called after components start and before readiness checks.
    #[must_use]
    pub fn on_start<F, Fut>(self, hook: F) -> Self
    where
        F: Fn(CancellationToken) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = AppResult<()>> + Send + 'static,
    {
        self.register_event_hook(LifecyclePhase::Start, make_hook(hook))
    }

    /// Register a hook called after readiness checks and before the app is ready.
    #[must_use]
    pub fn on_ready<F, Fut>(self, hook: F) -> Self
    where
        F: Fn(CancellationToken) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = AppResult<()>> + Send + 'static,
    {
        self.register_event_hook(LifecyclePhase::Ready, make_hook(hook))
    }

    /// Register a hook called during graceful shutdown before components stop.
    #[must_use]
    pub fn on_stop<F, Fut>(self, hook: F) -> Self
    where
        F: Fn(CancellationToken) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = AppResult<()>> + Send + 'static,
    {
        self.register_event_hook(LifecyclePhase::Stop, make_hook(hook))
    }

    fn register_event_hook(self, phase: LifecyclePhase, hook: AsyncHook) -> Self {
        register_lifecycle_hook(&self.hooks, phase, hook);
        self
    }

    /// Return a clone of the shared application configuration.
    #[must_use]
    pub fn config(&self) -> Arc<C> {
        Arc::clone(&self.config)
    }

    /// Clone the shutdown token to share with long-running tasks.
    #[must_use]
    pub fn shutdown_token(&self) -> CancellationToken {
        self.shutdown_token.clone()
    }

    /// Run as a long-running service.
    pub async fn run(self) -> AppResult<()> {
        self.startup().await?;

        tracing::debug!("service ready — waiting for shutdown signal");
        wait_for_signal(self.shutdown_token.clone()).await;

        self.graceful_shutdown().await
    }

    /// Run a finite task (CLI tools / batch jobs).
    pub async fn run_task<F, Fut>(self, task: F) -> AppResult<()>
    where
        F: FnOnce(Arc<C>, CancellationToken) -> Fut,
        Fut: std::future::Future<Output = AppResult<()>>,
    {
        self.startup().await?;

        let task_result = tokio::select! {
            result = task(Arc::clone(&self.config), self.shutdown_token.clone()) => result,
            _ = wait_for_signal_owned(self.shutdown_token.clone()) => {
                tracing::info!("task cancelled by signal");
                Ok(())
            }
        };

        let stop_hook_result = self.emit_lifecycle_hooks(LifecyclePhase::Stop).await;
        let stop_components_result = self.registry.stop_all().await;
        if let Err(error) = &stop_components_result {
            tracing::warn!(error = %error, "component shutdown error");
        }

        if task_result.is_ok() {
            stop_hook_result?;
            stop_components_result?;
        }

        task_result
    }

    async fn startup(&self) -> AppResult<()> {
        crate::summary::print_startup(self.config.service_config(), self.registry.len());
        self.configure().await?;

        if let Err(error) = self.registry.start_all().await {
            return Err(self
                .rollback_startup_failure(error.context("component startup failed"), false)
                .await);
        }

        if let Err(error) = self.emit_lifecycle_hooks(LifecyclePhase::Start).await {
            return Err(self
                .rollback_startup_failure(error.context("startup hooks failed"), true)
                .await);
        }

        if let Err(error) = self.ready_check() {
            tracing::warn!(error = %error, "ready check reported issues");
        }

        if let Err(error) = self.emit_lifecycle_hooks(LifecyclePhase::Ready).await {
            return Err(self
                .rollback_startup_failure(error.context("ready hooks failed"), true)
                .await);
        }

        Ok(())
    }

    async fn configure(&self) -> AppResult<()> {
        for hook in &self.on_configure {
            hook(self.shutdown_token.clone()).await?;
        }
        Ok(())
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

    async fn emit_lifecycle_hooks(&self, phase: LifecyclePhase) -> AppResult<()> {
        let hooks = Arc::clone(&self.hooks);
        let handle = tokio::runtime::Handle::current();
        let cancel = self.shutdown_token.clone();
        let result = tokio::task::spawn_blocking(move || match phase {
            LifecyclePhase::Start => hooks.emit(
                &AppStarted {
                    runtime_handle: handle,
                },
                cancel,
            ),
            LifecyclePhase::Ready => hooks.emit(
                &AppReady {
                    runtime_handle: handle,
                },
                cancel,
            ),
            LifecyclePhase::Stop => hooks.emit(
                &AppStopping {
                    runtime_handle: handle,
                },
                cancel,
            ),
        })
        .await
        .map_err(AppError::internal)?;
        hook_result_to_error(phase, result)
    }

    async fn rollback_startup_failure(
        &self,
        mut error: AppError,
        stop_components: bool,
    ) -> AppError {
        if let Err(stop_hook_error) = self.emit_lifecycle_hooks(LifecyclePhase::Stop).await {
            tracing::warn!(error = %stop_hook_error, "stop hook error during startup rollback");
            error = error.context(format!(
                "startup rollback stop hooks failed: {stop_hook_error}"
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

    async fn graceful_shutdown(self) -> AppResult<()> {
        let stop_result = tokio::time::timeout(
            self.graceful_timeout,
            self.emit_lifecycle_hooks(LifecyclePhase::Stop),
        )
        .await;

        match stop_result {
            Ok(Ok(())) => {}
            Ok(Err(error)) => tracing::warn!(error = %error, "stop hook error"),
            Err(_) => tracing::warn!("graceful shutdown timeout — forcing stop"),
        }

        if let Err(error) = self.registry.stop_all().await {
            tracing::warn!(error = %error, "component shutdown error");
        }
        tracing::info!("shutdown complete");
        Ok(())
    }
}

async fn wait_for_signal(token: CancellationToken) {
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            tracing::info!("received SIGINT");
        }
        _ = token.cancelled() => {
            tracing::info!("shutdown token cancelled");
        }
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
    use rskit_config::ServiceConfig;
    use rskit_hook::HookError;
    use tokio_util::sync::CancellationToken;

    use super::{AppBuilder, hook_result_to_error};
    use crate::hooks::LifecyclePhase;

    #[derive(Debug, Default, serde::Deserialize, validator::Validate)]
    struct TestCfg {
        #[serde(default)]
        service: rskit_config::ServiceConfig,
    }

    impl rskit_config::AppConfig for TestCfg {
        fn apply_defaults(&mut self) {}

        fn service_config(&self) -> &ServiceConfig {
            &self.service
        }
    }

    #[tokio::test]
    async fn app_builder_builds_successfully() {
        let cfg = TestCfg::default();
        let result = AppBuilder::new(cfg).build();
        assert!(
            result.is_ok(),
            "AppBuilder::build should succeed with a valid config"
        );
    }

    #[tokio::test]
    async fn builder_lifecycle_hooks_run_in_order() {
        let cfg = TestCfg::default();
        let order = Arc::new(Mutex::new(Vec::new()));
        let start_order = Arc::clone(&order);
        let ready_order = Arc::clone(&order);
        let stop_order = Arc::clone(&order);

        let app = AppBuilder::new(cfg)
            .on_start(move |_token| {
                let order = Arc::clone(&start_order);
                async move {
                    order.lock().push("start");
                    Ok(())
                }
            })
            .on_ready(move |_token| {
                let order = Arc::clone(&ready_order);
                async move {
                    order.lock().push("ready");
                    Ok(())
                }
            })
            .on_stop(move |_token| {
                let order = Arc::clone(&stop_order);
                async move {
                    order.lock().push("stop");
                    Ok(())
                }
            })
            .build()
            .expect("build should succeed");

        let result = app
            .run_task(|_cfg: Arc<TestCfg>, _cancel: CancellationToken| async move { Ok(()) })
            .await;

        assert!(result.is_ok(), "run_task should complete with Ok(())");
        assert_eq!(*order.lock(), vec!["start", "ready", "stop"]);
    }

    #[tokio::test]
    async fn app_run_task_executes_and_exits() {
        let cfg = TestCfg::default();
        let app = AppBuilder::new(cfg).build().expect("build should succeed");
        let runs = Arc::new(AtomicUsize::new(0));
        let run_counter = Arc::clone(&runs);

        let result = app
            .run_task(move |_cfg: Arc<TestCfg>, _cancel: CancellationToken| {
                let runs = Arc::clone(&run_counter);
                async move {
                    runs.fetch_add(1, Ordering::SeqCst);
                    Ok::<(), rskit_errors::AppError>(())
                }
            })
            .await;

        assert!(result.is_ok(), "run_task should complete with Ok(())");
        assert_eq!(runs.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn non_fatal_hook_error_is_logged_and_ignored() {
        let result = hook_result_to_error(LifecyclePhase::Start, Err(HookError::new("warn only")));

        assert!(result.is_ok());
    }

    #[test]
    fn fatal_hook_error_becomes_app_error() {
        let result =
            hook_result_to_error(LifecyclePhase::Start, Err(HookError::fatal("hard fail")));

        let error = result.expect_err("fatal hook error should fail");
        assert!(error.to_string().contains("on_start hook failed"));
    }
}
