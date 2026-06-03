use std::collections::HashMap;
use std::marker::PhantomData;
use std::sync::Arc;

use async_trait::async_trait;
use rskit_errors::AppResult;
use rskit_provider::{Provider, RequestResponse};
use tokio_util::sync::CancellationToken;

use crate::Dag;

/// Configuration for wrapping a DAG as a [`RequestResponse`] provider.
pub struct ToolConfig<I, O> {
    /// Provider name.
    pub name: &'static str,
    /// Maps input to initial inputs for root DAG nodes.
    pub input_fn: Box<dyn Fn(I) -> HashMap<String, serde_json::Value> + Send + Sync>,
    /// Extracts output from DAG execution results.
    pub output_fn: Box<dyn Fn(HashMap<String, serde_json::Value>) -> AppResult<O> + Send + Sync>,
}

/// Wraps a DAG execution as a [`RequestResponse`] provider.
pub struct DagTool<I, O> {
    dag: Arc<Dag>,
    config: ToolConfig<I, O>,
    _phantom: PhantomData<fn(I) -> O>,
}

/// Wrap a DAG execution as a [`RequestResponse`] provider.
///
/// The returned [`DagTool`] maps an input of type `I` into initial DAG inputs
/// via `config.input_fn`, executes the DAG, then extracts an output of type `O`
/// via `config.output_fn`.
pub fn as_tool<I, O>(dag: Arc<Dag>, config: ToolConfig<I, O>) -> DagTool<I, O>
where
    I: Send + 'static,
    O: Send + 'static,
{
    DagTool {
        dag,
        config,
        _phantom: PhantomData,
    }
}

impl<I: Send + 'static, O: Send + 'static> Provider for DagTool<I, O> {
    fn name(&self) -> &'static str {
        self.config.name
    }
}

#[async_trait]
impl<I, O> RequestResponse<I, O> for DagTool<I, O>
where
    I: Send + 'static,
    O: Send + 'static,
{
    async fn execute(&self, input: I) -> AppResult<O> {
        let initial_inputs = (self.config.input_fn)(input);
        let cancel = CancellationToken::new();
        let outputs = self.dag.execute_with_inputs(initial_inputs, cancel).await?;
        (self.config.output_fn)(outputs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::DagNode;
    use rskit_errors::{AppError, ErrorCode};
    use std::future::Future;
    use std::pin::Pin;

    struct EchoNode {
        id: String,
    }

    impl DagNode for EchoNode {
        fn id(&self) -> &str {
            &self.id
        }

        fn execute(
            &self,
            inputs: HashMap<String, serde_json::Value>,
            _cancel: CancellationToken,
        ) -> Pin<Box<dyn Future<Output = AppResult<serde_json::Value>> + Send + '_>> {
            Box::pin(async move { Ok(serde_json::to_value(&inputs).unwrap()) })
        }
    }

    struct AddNode {
        id: String,
        value: i64,
    }

    impl DagNode for AddNode {
        fn id(&self) -> &str {
            &self.id
        }

        fn execute(
            &self,
            inputs: HashMap<String, serde_json::Value>,
            _cancel: CancellationToken,
        ) -> Pin<Box<dyn Future<Output = AppResult<serde_json::Value>> + Send + '_>> {
            Box::pin(async move {
                let sum: i64 = inputs.values().filter_map(|v| v.as_i64()).sum::<i64>() + self.value;
                Ok(serde_json::json!(sum))
            })
        }
    }

    struct FailNode {
        id: String,
    }

    impl DagNode for FailNode {
        fn id(&self) -> &str {
            &self.id
        }

        fn execute(
            &self,
            _inputs: HashMap<String, serde_json::Value>,
            _cancel: CancellationToken,
        ) -> Pin<Box<dyn Future<Output = AppResult<serde_json::Value>> + Send + '_>> {
            Box::pin(async { Err(AppError::new(ErrorCode::Internal, "node failed")) })
        }
    }

    #[tokio::test]
    async fn execute_with_inputs_root_receives_inputs() {
        let dag = Dag::new().add_node(EchoNode { id: "root".into() });

        let cancel = CancellationToken::new();
        let mut initial = HashMap::new();
        initial.insert("x".to_owned(), serde_json::json!(42));

        let outputs = dag.execute_with_inputs(initial, cancel).await.unwrap();
        let root_output: HashMap<String, serde_json::Value> =
            serde_json::from_value(outputs["root"].clone()).unwrap();
        assert_eq!(root_output["x"], serde_json::json!(42));
    }

    #[tokio::test]
    async fn as_tool_basic_execution() {
        let dag = Arc::new(
            Dag::new()
                .add_node(AddNode {
                    id: "a".into(),
                    value: 0,
                })
                .add_node(AddNode {
                    id: "b".into(),
                    value: 5,
                })
                .add_edge("a", "b")
                .unwrap(),
        );

        let tool = as_tool(
            dag,
            ToolConfig {
                name: "add-pipeline",
                input_fn: Box::new(|input: i64| {
                    let mut m = HashMap::new();
                    m.insert("seed".to_owned(), serde_json::json!(input));
                    m
                }),
                output_fn: Box::new(|outputs| {
                    outputs["b"]
                        .as_i64()
                        .ok_or_else(|| AppError::new(ErrorCode::Internal, "missing b"))
                }),
            },
        );

        let result = RequestResponse::execute(&tool, 10).await.unwrap();
        // a receives {"seed": 10} → sums to 10, b receives {"a": 10} → 10 + 5 = 15
        assert_eq!(result, 15);
    }

    #[tokio::test]
    async fn as_tool_provider_name() {
        let dag = Arc::new(Dag::new().add_node(EchoNode { id: "n".into() }));

        let tool: DagTool<(), ()> = as_tool(
            dag,
            ToolConfig {
                name: "my-tool",
                input_fn: Box::new(|_| HashMap::new()),
                output_fn: Box::new(|_| Ok(())),
            },
        );

        assert_eq!(Provider::name(&tool), "my-tool");
    }

    #[tokio::test]
    async fn as_tool_error_propagation() {
        let dag = Arc::new(Dag::new().add_node(FailNode { id: "fail".into() }));

        let tool = as_tool(
            dag,
            ToolConfig {
                name: "failing",
                input_fn: Box::new(|_: ()| HashMap::new()),
                output_fn: Box::new(|_| Ok(())),
            },
        );

        let result = RequestResponse::execute(&tool, ()).await;
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().code(), ErrorCode::Internal);
    }
}
