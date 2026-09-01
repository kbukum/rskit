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
    pub(crate) configure: Vec<AsyncHook>,
    pub(crate) before_start: Vec<AsyncHook>,
    pub(crate) after_start: Vec<AsyncHook>,
    pub(crate) ready: Vec<AsyncHook>,
    pub(crate) before_stop: Vec<AsyncHook>,
    pub(crate) after_stop: Vec<AsyncHook>,
}

#[derive(Debug, Clone, Copy)]
pub(crate) enum LifecyclePhase {
    Configure,
    BeforeStart,
    AfterStart,
    Ready,
    BeforeStop,
    AfterStop,
}

impl LifecyclePhase {
    const fn label(self) -> &'static str {
        match self {
            Self::Configure => "configure",
            Self::BeforeStart => "before_start",
            Self::AfterStart => "after_start",
            Self::Ready => "ready",
            Self::BeforeStop => "before_stop",
            Self::AfterStop => "after_stop",
        }
    }

    const fn event_type(self) -> LifecycleEventType {
        match self {
            Self::Configure => LifecycleEventType::Configure,
            Self::BeforeStart => LifecycleEventType::BeforeStart,
            Self::AfterStart => LifecycleEventType::AfterStart,
            Self::Ready => LifecycleEventType::Ready,
            Self::BeforeStop => LifecycleEventType::BeforeStop,
            Self::AfterStop => LifecycleEventType::AfterStop,
        }
    }

    const fn fail_when_cancelled(self) -> bool {
        matches!(
            self,
            Self::Configure | Self::BeforeStart | Self::AfterStart | Self::Ready
        )
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

#[cfg(test)]
mod tests {
    use rskit_hook::{EventBus, EventBusConfig};

    use super::*;

    #[test]
    fn lifecycle_phase_metadata_covers_all_variants() {
        assert_eq!(LifecyclePhase::Configure.label(), "configure");
        assert_eq!(LifecyclePhase::BeforeStart.label(), "before_start");
        assert_eq!(LifecyclePhase::AfterStart.label(), "after_start");
        assert_eq!(LifecyclePhase::Ready.label(), "ready");
        assert_eq!(LifecyclePhase::BeforeStop.label(), "before_stop");
        assert_eq!(LifecyclePhase::AfterStop.label(), "after_stop");
        assert_eq!(
            LifecyclePhase::AfterStop.event_type(),
            LifecycleEventType::AfterStop
        );
        assert!(LifecyclePhase::Configure.fail_when_cancelled());
        assert!(LifecyclePhase::Ready.fail_when_cancelled());
        assert!(LifecyclePhase::BeforeStart.fail_when_cancelled());
        assert!(!LifecyclePhase::BeforeStop.fail_when_cancelled());
    }

    #[tokio::test]
    async fn run_hooks_rejects_cancelled_start_but_allows_cancelled_stop() {
        let bus = EventBus::new(EventBusConfig::default());
        let token = CancellationToken::new();
        token.cancel();
        let hook = make_hook(|_| async { Ok(()) });

        let err = run_hooks(
            LifecyclePhase::BeforeStart,
            std::slice::from_ref(&hook),
            token.clone(),
            &bus,
        )
        .await
        .unwrap_err();
        assert_eq!(err.code(), ErrorCode::Cancelled);

        run_hooks(LifecyclePhase::BeforeStop, &[hook], token, &bus)
            .await
            .unwrap();
    }

    #[test]
    fn record_stop_error_keeps_first_error_and_adds_context() {
        let mut stop_error = None;
        record_stop_error(
            &mut stop_error,
            AppError::new(ErrorCode::Internal, "first"),
            "first context",
        );
        record_stop_error(
            &mut stop_error,
            AppError::new(ErrorCode::Internal, "second"),
            "second context",
        );

        let error = stop_error.unwrap();
        assert!(error.to_string().contains("first"));
        assert!(error.to_string().contains("second context"));
    }
}
