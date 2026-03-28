use std::marker::PhantomData;
use std::sync::Arc;
use std::time::Duration;

use futures_util::future::BoxFuture;
use rskit_config::AppConfig;
use rskit_errors::{AppError, AppResult};
use tokio_util::sync::CancellationToken;

use crate::{Component, Registry};

// ─── Typestates ──────────────────────────────────────────────────────────────

/// Typestate marker: App not yet started.
pub struct Unconfigured;

// ─── Hook type ───────────────────────────────────────────────────────────────

type Hook = Arc<
    dyn Fn(CancellationToken) -> BoxFuture<'static, AppResult<()>> + Send + Sync + 'static,
>;

fn make_hook<F, Fut>(f: F) -> Hook
where
    F: Fn(CancellationToken) -> Fut + Send + Sync + 'static,
    Fut: std::future::Future<Output = AppResult<()>> + Send + 'static,
{
    Arc::new(move |tok| Box::pin(f(tok)))
}

async fn run_hooks(hooks: &[Hook], token: CancellationToken) -> AppResult<()> {
    for hook in hooks {
        hook(token.clone()).await?;
    }
    Ok(())
}

// ─── AppBuilder ───────────────────────────────────────────────────────────────

/// Builder for [`App`]. Validates config before handing off to the lifecycle.
pub struct AppBuilder<C: AppConfig> {
    config: C,
    graceful_timeout: Duration,
    components: Vec<Arc<dyn Component>>,
}

impl<C: AppConfig> AppBuilder<C> {
    pub fn new(config: C) -> Self {
        Self {
            config,
            graceful_timeout: Duration::from_secs(30),
            components: Vec::new(),
        }
    }

    pub fn with_graceful_timeout(mut self, t: Duration) -> Self {
        self.graceful_timeout = t;
        self
    }

    pub fn with_component(mut self, c: Arc<dyn Component>) -> Self {
        self.components.push(c);
        self
    }

    /// Validate config and build the App.
    pub fn build(self) -> AppResult<App<Unconfigured, C>> {
        use validator::Validate;
        self.config
            .validate()
            .map_err(|e| AppError::invalid_input("config", e.to_string()))?;

        let mut registry = Registry::new();
        for c in self.components {
            registry.register(c);
        }

        Ok(App {
            _state: PhantomData,
            config: Arc::new(self.config),
            registry,
            on_configure: Vec::new(),
            on_start: Vec::new(),
            on_ready: Vec::new(),
            on_stop: Vec::new(),
            graceful_timeout: self.graceful_timeout,
            shutdown_token: CancellationToken::new(),
        })
    }
}

// ─── App ─────────────────────────────────────────────────────────────────────

/// Application orchestrator with typestate lifecycle.
///
/// # Lifecycle
///
/// ```text
/// build()
///   → start_all components
///   → on_configure hooks
///   → on_start hooks
///   → on_ready hooks
///   → wait for SIGINT/SIGTERM or manual cancel
///   → on_stop hooks
///   → stop_all components (LIFO)
/// ```
pub struct App<S, C> {
    _state: PhantomData<S>,
    config: Arc<C>,
    registry: Registry,
    on_configure: Vec<Hook>,
    on_start: Vec<Hook>,
    on_ready: Vec<Hook>,
    on_stop: Vec<Hook>,
    graceful_timeout: Duration,
    shutdown_token: CancellationToken,
}

impl<C: AppConfig> App<Unconfigured, C> {
    /// Convenience: creates an [`AppBuilder`].
    pub fn builder(config: C) -> AppBuilder<C> {
        AppBuilder::new(config)
    }

    // ── Hook registration ─────────────────────────────────────────────

    pub fn on_configure<F, Fut>(mut self, f: F) -> Self
    where
        F: Fn(CancellationToken) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = AppResult<()>> + Send + 'static,
    {
        self.on_configure.push(make_hook(f));
        self
    }

    pub fn on_start<F, Fut>(mut self, f: F) -> Self
    where
        F: Fn(CancellationToken) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = AppResult<()>> + Send + 'static,
    {
        self.on_start.push(make_hook(f));
        self
    }

    pub fn on_ready<F, Fut>(mut self, f: F) -> Self
    where
        F: Fn(CancellationToken) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = AppResult<()>> + Send + 'static,
    {
        self.on_ready.push(make_hook(f));
        self
    }

    pub fn on_stop<F, Fut>(mut self, f: F) -> Self
    where
        F: Fn(CancellationToken) -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = AppResult<()>> + Send + 'static,
    {
        self.on_stop.push(make_hook(f));
        self
    }

    // ── Accessors ─────────────────────────────────────────────────────

    pub fn config(&self) -> Arc<C> {
        self.config.clone()
    }

    /// Clone the shutdown token to share with components that need to react
    /// to graceful shutdown (e.g. long-running tasks).
    pub fn shutdown_token(&self) -> CancellationToken {
        self.shutdown_token.clone()
    }

    // ── Execution ─────────────────────────────────────────────────────

    /// Run as a long-running service.
    ///
    /// Blocks until a shutdown signal (SIGINT/SIGTERM) or
    /// [`shutdown_token`](Self::shutdown_token) is cancelled.
    pub async fn run(self) -> AppResult<()> {
        crate::summary::print_startup(self.config.service_config(), self.registry.len());

        self.registry.start_all().await?;
        run_hooks(&self.on_configure, self.shutdown_token.clone()).await?;
        run_hooks(&self.on_start, self.shutdown_token.clone()).await?;
        run_hooks(&self.on_ready, self.shutdown_token.clone()).await?;

        tracing::info!("service ready — waiting for shutdown signal");
        wait_for_signal(self.shutdown_token.clone()).await;

        self.graceful_shutdown().await
    }

    /// Run a finite task (CLI tools / batch jobs).
    ///
    /// Starts components, calls `task`, then shuts down cleanly.
    /// The task receives the shared config and a cancellation token it can
    /// monitor for early termination.
    pub async fn run_task<F, Fut>(self, task: F) -> AppResult<()>
    where
        F: FnOnce(Arc<C>, CancellationToken) -> Fut,
        Fut: std::future::Future<Output = AppResult<()>>,
    {
        self.registry.start_all().await?;
        run_hooks(&self.on_configure, self.shutdown_token.clone()).await?;
        run_hooks(&self.on_start, self.shutdown_token.clone()).await?;

        let result = tokio::select! {
            r = task(self.config.clone(), self.shutdown_token.clone()) => r,
            _ = wait_for_signal_owned(self.shutdown_token.clone()) => {
                tracing::info!("task cancelled by signal");
                Ok(())
            }
        };

        run_hooks(&self.on_stop, self.shutdown_token.clone()).await?;
        self.registry.stop_all().await;
        result
    }

    async fn graceful_shutdown(self) -> AppResult<()> {
        // Run stop hooks with a timeout
        let stop_result = tokio::time::timeout(
            self.graceful_timeout,
            run_hooks(&self.on_stop, self.shutdown_token.clone()),
        )
        .await;

        match stop_result {
            Ok(r) => {
                if let Err(e) = r {
                    tracing::warn!(error = %e, "stop hook error");
                }
            }
            Err(_) => {
                tracing::warn!("graceful shutdown timeout — forcing stop");
            }
        }

        self.registry.stop_all().await;
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
    wait_for_signal(token).await
}
