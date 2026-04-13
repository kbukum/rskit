//! Sequential chain execution pattern for rskit.
//!
//! Provides a simple, composable way to run a sequence of async operations
//! where each step receives the output of the previous step.  Supports
//! per-step progress reporting, cancellation at step boundaries, and
//! automatic cleanup of completed steps when a later step fails.
//!
//! # Quick start
//!
//! ```ignore
//! use rskit_chain::{ChainBuilder, ChainOperation};
//!
//! let chain = ChainBuilder::new()
//!     .step(MyFirstOp::new())
//!     .step(MySecondOp::new())
//!     .build();
//!
//! let result = chain.execute(input, None, cancel).await?;
//! ```

#![warn(missing_docs)]

/// Fluent builder for constructing chain executors.
pub mod builder;
/// Sequential chain executor.
pub mod executor;
/// [`ChainOperation`] trait for individual steps.
pub mod operation;
/// Result and progress types.
pub mod types;

pub use builder::ChainBuilder;
pub use executor::{ChainConfig, ChainExecutor, ChainProgressFn};
pub use operation::{ChainOperation, ProgressFn};
pub use types::{ChainResult, StepProgress, StepResult, StepStatus};

#[cfg(test)]
mod tests {
    use super::*;
    use rskit_errors::{AppError, AppResult, ErrorCode};
    use serde_json::{Value, json};
    use std::future::Future;
    use std::pin::Pin;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use tokio_util::sync::CancellationToken;

    // ── Mock operations ──────────────────────────────────────────────────

    struct IncrementOp {
        id: String,
    }

    impl IncrementOp {
        fn new(id: &str) -> Self {
            Self { id: id.to_string() }
        }
    }

    impl ChainOperation for IncrementOp {
        fn id(&self) -> &str {
            &self.id
        }

        fn execute(
            &self,
            input: Value,
            progress: ProgressFn,
            _cancel: CancellationToken,
        ) -> Pin<Box<dyn Future<Output = AppResult<Value>> + Send + '_>> {
            Box::pin(async move {
                let n = input.as_i64().unwrap_or(0);
                progress(50, Some("halfway".into()));
                progress(100, None);
                Ok(json!(n + 1))
            })
        }
    }

    struct FailOp {
        id: String,
    }

    impl FailOp {
        fn new(id: &str) -> Self {
            Self { id: id.to_string() }
        }
    }

    impl ChainOperation for FailOp {
        fn id(&self) -> &str {
            &self.id
        }

        fn execute(
            &self,
            _input: Value,
            _progress: ProgressFn,
            _cancel: CancellationToken,
        ) -> Pin<Box<dyn Future<Output = AppResult<Value>> + Send + '_>> {
            Box::pin(async {
                Err(AppError::new(ErrorCode::Internal, "intentional failure"))
            })
        }
    }

    struct CleanupTracker {
        id: String,
        cleaned: Arc<AtomicBool>,
    }

    impl CleanupTracker {
        fn new(id: &str, cleaned: Arc<AtomicBool>) -> Self {
            Self {
                id: id.to_string(),
                cleaned,
            }
        }
    }

    impl ChainOperation for CleanupTracker {
        fn id(&self) -> &str {
            &self.id
        }

        fn execute(
            &self,
            input: Value,
            _progress: ProgressFn,
            _cancel: CancellationToken,
        ) -> Pin<Box<dyn Future<Output = AppResult<Value>> + Send + '_>> {
            Box::pin(async move { Ok(input) })
        }

        fn cleanup(&self, _output: &Value) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
            let cleaned = self.cleaned.clone();
            Box::pin(async move {
                cleaned.store(true, Ordering::SeqCst);
            })
        }
    }

    // ── Tests ────────────────────────────────────────────────────────────

    /// Simple 3-step chain that increments a number.
    #[tokio::test]
    async fn test_simple_chain_increments() {
        let chain = ChainBuilder::new()
            .step(IncrementOp::new("step-1"))
            .step(IncrementOp::new("step-2"))
            .step(IncrementOp::new("step-3"))
            .build();

        let cancel = CancellationToken::new();
        let result = chain.execute(json!(0), None, cancel).await.unwrap();

        assert!(result.success);
        assert_eq!(result.completed_steps(), 3);
        assert_eq!(result.final_output, Some(json!(3)));
        assert!(result.failed_step().is_none());

        // Verify each step result
        for (i, step) in result.steps.iter().enumerate() {
            assert_eq!(step.status, StepStatus::Completed);
            assert_eq!(step.output, json!(i as i64 + 1));
        }
    }

    /// Chain with failure in the middle step — verify cleanup runs on completed steps.
    #[tokio::test]
    async fn test_failure_triggers_cleanup() {
        let cleaned_1 = Arc::new(AtomicBool::new(false));
        let cleaned_2 = Arc::new(AtomicBool::new(false));

        let chain = ChainBuilder::new()
            .step(CleanupTracker::new("tracker-1", cleaned_1.clone()))
            .step(FailOp::new("fail-op"))
            .step(CleanupTracker::new("tracker-2", cleaned_2.clone()))
            .cleanup_on_failure(true)
            .stop_on_failure(true)
            .build();

        let cancel = CancellationToken::new();
        let result = chain.execute(json!(null), None, cancel).await.unwrap();

        assert!(!result.success);
        assert_eq!(result.completed_steps(), 1);
        assert!(result.final_output.is_none());

        // Step 0 completed, step 1 failed, step 2 skipped
        assert_eq!(result.steps[0].status, StepStatus::Completed);
        assert_eq!(result.steps[1].status, StepStatus::Failed);
        assert_eq!(result.steps[2].status, StepStatus::Skipped);

        // Cleanup should have run on the completed tracker-1
        assert!(cleaned_1.load(Ordering::SeqCst));
        // tracker-2 was skipped, so no cleanup
        assert!(!cleaned_2.load(Ordering::SeqCst));
    }

    /// Chain with cancellation — verify remaining steps are marked cancelled.
    #[tokio::test]
    async fn test_cancellation_marks_remaining_cancelled() {
        let cancel = CancellationToken::new();

        // Cancel before the chain runs
        cancel.cancel();

        let chain = ChainBuilder::new()
            .step(IncrementOp::new("step-1"))
            .step(IncrementOp::new("step-2"))
            .step(IncrementOp::new("step-3"))
            .build();

        let result = chain.execute(json!(0), None, cancel).await.unwrap();

        assert!(!result.success);
        assert_eq!(result.completed_steps(), 0);

        for step in &result.steps {
            assert_eq!(step.status, StepStatus::Cancelled);
            assert_eq!(step.error.as_deref(), Some("chain cancelled"));
        }
    }

    /// Chain with stop_on_failure=false — verify all steps run even after failure.
    #[tokio::test]
    async fn test_continue_after_failure() {
        let chain = ChainBuilder::new()
            .step(IncrementOp::new("step-1"))
            .step(FailOp::new("fail-op"))
            .step(IncrementOp::new("step-3"))
            .stop_on_failure(false)
            .cleanup_on_failure(false)
            .build();

        let cancel = CancellationToken::new();
        let result = chain.execute(json!(0), None, cancel).await.unwrap();

        assert!(!result.success);
        // step-1 completed, fail-op failed, step-3 still ran
        assert_eq!(result.steps[0].status, StepStatus::Completed);
        assert_eq!(result.steps[1].status, StepStatus::Failed);
        assert_eq!(result.steps[2].status, StepStatus::Completed);
        assert_eq!(result.completed_steps(), 2);
    }

    /// Progress callback verification — capture all progress events and validate order.
    #[tokio::test]
    async fn test_progress_callback_events() {
        let events = Arc::new(std::sync::Mutex::new(Vec::<StepProgress>::new()));

        let chain = ChainBuilder::new()
            .step(IncrementOp::new("step-1"))
            .step(IncrementOp::new("step-2"))
            .build();

        let cancel = CancellationToken::new();
        let events_clone = events.clone();
        let progress: ChainProgressFn = Arc::new(move |p: StepProgress| {
            events_clone.lock().unwrap().push(p);
        });

        let result = chain
            .execute(json!(0), Some(progress), cancel)
            .await
            .unwrap();

        assert!(result.success);

        let captured = events.lock().unwrap();

        // For each step we expect: Running(0%), Running(50%), Running(100%), Completed(100%)
        // That's 4 events per step × 2 steps = 8 total
        assert_eq!(captured.len(), 8);

        // First step events
        assert_eq!(captured[0].step_id, "step-1");
        assert_eq!(captured[0].status, StepStatus::Running);
        assert_eq!(captured[0].progress_percent, 0);

        assert_eq!(captured[1].step_id, "step-1");
        assert_eq!(captured[1].status, StepStatus::Running);
        assert_eq!(captured[1].progress_percent, 50);

        assert_eq!(captured[2].step_id, "step-1");
        assert_eq!(captured[2].status, StepStatus::Running);
        assert_eq!(captured[2].progress_percent, 100);

        assert_eq!(captured[3].step_id, "step-1");
        assert_eq!(captured[3].status, StepStatus::Completed);
        assert_eq!(captured[3].progress_percent, 100);

        // Second step events
        assert_eq!(captured[4].step_id, "step-2");
        assert_eq!(captured[4].status, StepStatus::Running);
        assert_eq!(captured[4].progress_percent, 0);

        assert_eq!(captured[7].step_id, "step-2");
        assert_eq!(captured[7].status, StepStatus::Completed);
        assert_eq!(captured[7].progress_percent, 100);
    }
}
