use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;

use rskit_errors::AppResult;
use tokio_util::sync::CancellationToken;

/// A single executable node in a DAG pipeline.
pub trait DagNode: Send + Sync + 'static {
    /// Unique identifier for this node within the DAG.
    fn id(&self) -> &str;

    /// Execute the node with the collected outputs from upstream dependencies.
    fn execute(
        &self,
        inputs: HashMap<String, serde_json::Value>,
        cancel: CancellationToken,
    ) -> Pin<Box<dyn Future<Output = AppResult<serde_json::Value>> + Send + '_>>;
}
