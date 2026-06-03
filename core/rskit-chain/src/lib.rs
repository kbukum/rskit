//! Typed sequential chain execution for rskit.
//!
//! A chain is a statically typed sequence where each step receives the previous
//! step's output and returns the next step's input type. Execution short-circuits
//! on the first [`AppError`](rskit_errors::AppError), checks cancellation between
//! steps, and runs registered cleanup actions for already-completed steps when
//! a later failure or cancellation interrupts the chain.
//!
//! # Quick start
//!
//! ```
//! use rskit_chain::{ChainBuilder, Step};
//! use tokio_util::sync::CancellationToken;
//!
//! # async fn run() -> rskit_errors::AppResult<()> {
//! let chain = ChainBuilder::new()
//!     .step(Step::from_fn("parse", |input: String, _ctx| async move {
//!         input.parse::<u32>().map_err(|err| {
//!             rskit_errors::AppError::invalid_input("input", err.to_string())
//!         })
//!     }))
//!     .step(Step::from_fn("double", |value: u32, _ctx| async move {
//!         Ok(value * 2)
//!     }))
//!     .build();
//!
//! let output = chain
//!     .execute("21".to_string(), None, CancellationToken::new())
//!     .await?;
//! assert_eq!(output, 42);
//! # Ok(())
//! # }
//! ```

#![warn(missing_docs)]

/// Fluent builder for constructing typed chain executors.
pub mod builder;
/// Typed sequential chain executor.
pub mod executor;
/// Typed step primitives.
pub mod operation;
/// Progress types.
pub mod types;

pub use builder::ChainBuilder;
pub use executor::{Chain, ChainProgressFn};
pub use operation::{Step, StepContext, StepFuture};
pub use types::{StepProgress, StepStatus};

#[cfg(test)]
mod tests {
    use super::*;
    use parking_lot::Mutex;
    use rskit_errors::{AppError, AppResult, ErrorCode};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};
    use tokio_util::sync::CancellationToken;

    #[tokio::test]
    async fn typed_chain_composes_heterogeneous_steps() {
        let chain = ChainBuilder::new()
            .step(Step::from_fn(
                "parse",
                |input: String, context| async move {
                    context.progress(50, Some("parsed input".into()));
                    input
                        .parse::<u32>()
                        .map_err(|err| AppError::invalid_input("input", err.to_string()))
                },
            ))
            .step(Step::from_fn("format", |value: u32, _context| async move {
                Ok::<_, AppError>(format!("value={}", value + 1))
            }))
            .build();

        let output = chain
            .execute("41".to_string(), None, CancellationToken::new())
            .await
            .unwrap();

        assert_eq!(output, "value=42");
        assert_eq!(chain.len(), 2);
    }

    #[tokio::test]
    async fn failure_short_circuits_and_runs_cleanup() {
        let cleaned = Arc::new(AtomicBool::new(false));
        let cleanup_flag = cleaned.clone();

        let chain = ChainBuilder::new()
            .step(
                Step::from_fn("reserve", |value: u32, _context| async move {
                    Ok::<_, AppError>(value + 1)
                })
                .with_cleanup(move || {
                    let cleanup_flag = cleanup_flag.clone();
                    async move {
                        cleanup_flag.store(true, Ordering::SeqCst);
                        Ok(())
                    }
                }),
            )
            .step(Step::from_fn("fail", |_value: u32, _context| async move {
                Err::<u32, AppError>(AppError::new(ErrorCode::Internal, "intentional failure"))
            }))
            .step(Step::from_fn(
                "unreached",
                |value: u32, _context| async move { Ok::<_, AppError>(value + 1) },
            ))
            .build();

        let error = chain
            .execute(0u32, None, CancellationToken::new())
            .await
            .unwrap_err();

        assert_eq!(error.code(), ErrorCode::Internal);
        assert!(error.message().contains("fail"));
        assert!(cleaned.load(Ordering::SeqCst));
    }

    #[tokio::test]
    async fn cleanup_failure_does_not_hide_step_failure() {
        let chain = ChainBuilder::new()
            .step(
                Step::from_fn("reserve", |value: u32, _context| async move {
                    Ok::<_, AppError>(value + 1)
                })
                .with_cleanup(|| async {
                    Err::<(), AppError>(AppError::new(ErrorCode::Internal, "cleanup failure"))
                }),
            )
            .step(Step::from_fn("fail", |_value: u32, _context| async move {
                Err::<u32, AppError>(AppError::invalid_input("input", "primary failure"))
            }))
            .build();

        let error = chain
            .execute(0u32, None, CancellationToken::new())
            .await
            .unwrap_err();

        assert_eq!(error.code(), ErrorCode::InvalidInput);
        assert!(error.message().contains("chain step `fail` failed"));
        assert!(error.message().contains("primary failure"));
        assert!(error.message().contains("cleanup failure"));
    }

    #[tokio::test]
    async fn cancellation_before_next_step_fails_chain() {
        let cancel = CancellationToken::new();
        let cancel_for_step = cancel.clone();

        let chain = ChainBuilder::new()
            .step(Step::from_fn("cancel", move |value: u32, _context| {
                let cancel = cancel_for_step.clone();
                async move {
                    cancel.cancel();
                    Ok::<_, AppError>(value)
                }
            }))
            .step(Step::from_fn(
                "unreached",
                |value: u32, _context| async move { Ok::<_, AppError>(value + 1) },
            ))
            .build();

        let error = chain.execute(1u32, None, cancel).await.unwrap_err();
        assert_eq!(error.code(), ErrorCode::Cancelled);
    }

    #[tokio::test]
    async fn progress_callback_receives_step_events() {
        let events = Arc::new(Mutex::new(Vec::<StepProgress>::new()));
        let captured = events.clone();
        let progress: ChainProgressFn = Arc::new(move |event| {
            captured.lock().push(event);
        });

        let chain = ChainBuilder::new()
            .step(Step::from_fn("one", |value: u32, context| async move {
                context.progress(25, Some("working".into()));
                Ok::<_, AppError>(value + 1)
            }))
            .build();

        let output = chain
            .execute(1u32, Some(progress), CancellationToken::new())
            .await
            .unwrap();

        assert_eq!(output, 2);
        let events = events.lock();
        assert_eq!(events[0].status, StepStatus::Running);
        assert_eq!(events[1].progress_percent, 25);
        assert_eq!(events[2].status, StepStatus::Completed);
    }

    fn _assert_result_type(_: AppResult<u32>) {}
}
