use rskit_errors::AppResult;
use serde_json::Value;
use std::future::Future;
use std::pin::Pin;
use tokio_util::sync::CancellationToken;

/// Progress callback type for chain operations.
///
/// Receives a percentage (0–100) and an optional human-readable message.
pub type ProgressFn = Box<dyn Fn(u8, Option<String>) + Send + Sync>;

/// A single operation in a sequential chain.
///
/// Each operation receives the output of the previous operation (or the initial
/// input) as a JSON value, and produces a JSON value output for the next
/// operation.
pub trait ChainOperation: Send + Sync {
    /// Unique identifier for this operation.
    fn id(&self) -> &str;

    /// Human-readable name (defaults to [`id`](Self::id)).
    fn name(&self) -> &str {
        self.id()
    }

    /// Execute the operation.
    ///
    /// - `input`:    output from the previous step (or chain input for the first step)
    /// - `progress`: callback for progress updates (0–100 percent, optional message)
    /// - `cancel`:   token to check for cancellation between processing units
    fn execute(
        &self,
        input: Value,
        progress: ProgressFn,
        cancel: CancellationToken,
    ) -> Pin<Box<dyn Future<Output = AppResult<Value>> + Send + '_>>;

    /// Optional cleanup when the chain fails after this operation completed.
    ///
    /// Used to delete intermediate files, release resources, etc.
    fn cleanup(&self, _output: &Value) -> Pin<Box<dyn Future<Output = ()> + Send + '_>> {
        Box::pin(async {})
    }
}
