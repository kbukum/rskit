//! Typed sequential chain execution for rskit.
//!
//! A chain is a statically typed sequence where each step receives the previous step's output
//! and returns the next step's input type.
//! Execution short-circuits on the first [`AppError`](rskit_errors::AppError),
//! checks cancellation between steps,
//! and runs registered cleanup actions for already-completed steps when a later failure
//! or cancellation interrupts the chain.
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
mod tests;
