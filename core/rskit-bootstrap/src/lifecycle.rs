//! Lifecycle hook dispatch and shutdown helpers.

use std::sync::Arc;

use futures_util::future::BoxFuture;
use rskit_errors::{AppError, AppResult, ErrorCode};
use rskit_hook::EventBus;
use tokio_util::sync::CancellationToken;

use crate::hooks::{LifecycleEvent, LifecycleEventType};

pub(crate) type AsyncHook =
    Arc<dyn Fn(CancellationToken) -> BoxFuture<'static, AppResult<()>> + Send + Sync + 'static>;

pub(crate) fn make_hook<F, Fut>(f: F) -> AsyncHook
where
    F: Fn(CancellationToken) -> Fut + Send + Sync + 'static,
    Fut: std::future::Future<Output = AppResult<()>> + Send + 'static,
{
    Arc::new(move |token| Box::pin(f(token)))
}

#[derive(Default)]
pub(crate) struct LifecycleHooks {
    pub(crate) before_start: Vec<AsyncHook>,
    pub(crate) after_start: Vec<AsyncHook>,
    pub(crate) before_stop: Vec<AsyncHook>,
    pub(crate) after_stop: Vec<AsyncHook>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum LifecyclePhase {
    BeforeStart,
    AfterStart,
    BeforeStop,
    AfterStop,
}

impl LifecyclePhase {
    const fn label(self) -> &'static str {
        match self {
            Self::BeforeStart => "before_start",
            Self::AfterStart => "after_start",
            Self::BeforeStop => "before_stop",
            Self::AfterStop => "after_stop",
        }
    }

    const fn event_type(self) -> LifecycleEventType {
        match self {
            Self::BeforeStart => LifecycleEventType::BeforeStart,
            Self::AfterStart => LifecycleEventType::AfterStart,
            Self::BeforeStop => LifecycleEventType::BeforeStop,
            Self::AfterStop => LifecycleEventType::AfterStop,
        }
    }

    const fn fail_when_cancelled(self) -> bool {
        matches!(self, Self::BeforeStart | Self::AfterStart)
    }
}

pub(crate) async fn run_hooks(
    phase: LifecyclePhase,
    hooks: &[AsyncHook],
    token: CancellationToken,
    lifecycle_events: &EventBus<LifecycleEvent>,
) -> AppResult<()> {
    lifecycle_events.publish(LifecycleEvent::new(phase.event_type()))?;
    for hook in hooks {
        if phase.fail_when_cancelled() && token.is_cancelled() {
            return Err(AppError::new(
                ErrorCode::Cancelled,
                format!("{} hook dispatch cancelled", phase.label()),
            ));
        }
        hook(token.clone())
            .await
            .map_err(|error| error.context(format!("{} hook failed", phase.label())))?;
    }
    Ok(())
}

pub(crate) fn record_stop_error(stop_error: &mut Option<AppError>, error: AppError, context: &str) {
    if let Some(existing) = stop_error.take() {
        *stop_error = Some(existing.context(format!("{context}: {error}")));
    } else {
        *stop_error = Some(error);
    }
}

pub(crate) async fn wait_for_signal(token: CancellationToken) {
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {
            tracing::info!("received SIGINT");
        }
        _ = token.cancelled() => {
            tracing::info!("shutdown token cancelled");
        }
    }
}
