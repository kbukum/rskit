use std::collections::HashMap;
use std::future::Future;
use std::marker::PhantomData;
use std::pin::Pin;
use std::sync::Arc;

use rskit_errors::{AppError, AppResult, ErrorCode};
use serde::Serialize;
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

type InputMapper<I> = Arc<dyn Fn(HashMap<String, serde_json::Value>) -> AppResult<I> + Send + Sync>;

/// Typed adapter for a DAG node with explicit input and output contracts.
pub struct TypedDagNode<I, O, F> {
    id: String,
    map_inputs: InputMapper<I>,
    execute: F,
    _output: PhantomData<fn() -> O>,
}

impl<I, O, F> TypedDagNode<I, O, F> {
    /// Create a typed node from an input mapper and async operation.
    #[must_use]
    pub fn new(
        id: impl Into<String>,
        map_inputs: impl Fn(HashMap<String, serde_json::Value>) -> AppResult<I> + Send + Sync + 'static,
        execute: F,
    ) -> Self {
        Self {
            id: id.into(),
            map_inputs: Arc::new(map_inputs),
            execute,
            _output: PhantomData,
        }
    }
}

impl<I, O, F, Fut> DagNode for TypedDagNode<I, O, F>
where
    I: Send + 'static,
    O: Serialize + Send + 'static,
    F: Fn(I, CancellationToken) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = AppResult<O>> + Send + 'static,
{
    fn id(&self) -> &str {
        &self.id
    }

    fn execute(
        &self,
        inputs: HashMap<String, serde_json::Value>,
        cancel: CancellationToken,
    ) -> Pin<Box<dyn Future<Output = AppResult<serde_json::Value>> + Send + '_>> {
        Box::pin(async move {
            let input = (self.map_inputs)(inputs)?;
            let output = (self.execute)(input, cancel).await?;
            serde_json::to_value(output).map_err(|error| {
                AppError::new(
                    ErrorCode::Internal,
                    format!("failed to encode typed DAG node output: {error}"),
                )
            })
        })
    }
}
