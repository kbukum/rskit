use rskit_errors::AppResult;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use tokio_util::sync::CancellationToken;

/// Boxed future returned by chain steps.
pub type StepFuture<T> = Pin<Box<dyn Future<Output = AppResult<T>> + Send + 'static>>;

type StepProgressFn = Arc<dyn Fn(u8, Option<String>) + Send + Sync>;
type ExecuteFn<I, O> = dyn Fn(I, StepContext) -> StepFuture<O> + Send + Sync;
type CleanupFn = dyn Fn() -> StepFuture<()> + Send + Sync;

/// Per-step execution context.
#[derive(Clone)]
pub struct StepContext {
    cancel: CancellationToken,
    progress: Option<StepProgressFn>,
}

impl StepContext {
    pub(crate) fn new(cancel: CancellationToken, progress: Option<StepProgressFn>) -> Self {
        Self { cancel, progress }
    }

    /// Return the cancellation token shared by the chain execution.
    #[must_use]
    pub fn cancellation_token(&self) -> &CancellationToken {
        &self.cancel
    }

    /// Whether cancellation has been requested.
    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.cancel.is_cancelled()
    }

    /// Report step-local progress.
    pub fn progress(&self, percent: u8, message: Option<String>) {
        if let Some(progress) = &self.progress {
            progress(percent.min(100), message);
        }
    }
}

/// A typed operation in a sequential chain.
pub struct Step<I, O> {
    id: String,
    name: String,
    execute: Arc<ExecuteFn<I, O>>,
    cleanup: Option<Arc<CleanupFn>>,
}

impl<I, O> Step<I, O>
where
    I: Send + 'static,
    O: Send + 'static,
{
    /// Create a typed chain step.
    #[must_use]
    pub fn new<F, Fut>(id: impl Into<String>, name: impl Into<String>, execute: F) -> Self
    where
        F: Fn(I, StepContext) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = AppResult<O>> + Send + 'static,
    {
        Self {
            id: id.into(),
            name: name.into(),
            execute: Arc::new(move |input, context| Box::pin(execute(input, context))),
            cleanup: None,
        }
    }

    /// Create a typed chain step using `id` as the display name.
    #[must_use]
    pub fn from_fn<F, Fut>(id: impl Into<String>, execute: F) -> Self
    where
        F: Fn(I, StepContext) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = AppResult<O>> + Send + 'static,
    {
        let id = id.into();
        Self::new(id.clone(), id, execute)
    }

    /// Register cleanup to run only if a later step fails or cancellation interrupts the chain.
    #[must_use]
    pub fn with_cleanup<F, Fut>(mut self, cleanup: F) -> Self
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: Future<Output = AppResult<()>> + Send + 'static,
    {
        self.cleanup = Some(Arc::new(move || Box::pin(cleanup())));
        self
    }

    /// Unique step identifier.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Human-readable step name.
    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    pub(crate) fn execute(&self, input: I, context: StepContext) -> StepFuture<O> {
        (self.execute)(input, context)
    }

    pub(crate) fn cleanup(&self) -> Option<Arc<CleanupFn>> {
        self.cleanup.clone()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use parking_lot::Mutex;
    use std::sync::Arc;

    #[tokio::test]
    async fn step_context_reports_cancellation_and_clamps_progress() {
        let cancel = CancellationToken::new();
        let events = Arc::new(Mutex::new(Vec::new()));
        let captured = events.clone();
        let context = StepContext::new(
            cancel.clone(),
            Some(Arc::new(move |percent, message| {
                captured.lock().push((percent, message));
            })),
        );

        assert!(!context.is_cancelled());
        assert!(!context.cancellation_token().is_cancelled());
        context.progress(150, Some("too high".into()));
        cancel.cancel();
        assert!(context.is_cancelled());

        let events = events.lock();
        assert_eq!(events.as_slice(), &[(100, Some("too high".to_string()))]);
    }

    #[tokio::test]
    async fn step_accessors_execute_and_cleanup_work() {
        let step = Step::new("id", "display", |value: u32, _context| async move {
            Ok(value + 1)
        })
        .with_cleanup(|| async { Ok(()) });

        assert_eq!(step.id(), "id");
        assert_eq!(step.name(), "display");
        let output = step
            .execute(1, StepContext::new(CancellationToken::new(), None))
            .await
            .expect("step should run");
        assert_eq!(output, 2);
        step.cleanup().expect("cleanup should be registered")()
            .await
            .expect("cleanup should succeed");
    }
}
