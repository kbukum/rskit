use rskit_dag::{Dag, DagNode};
use rskit_errors::AppResult;
use serde_json::{Value, json};
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use tokio_util::sync::CancellationToken;

struct TestNode {
    name: String,
    value: i64,
}

impl TestNode {
    fn new(name: &str, value: i64) -> Self {
        Self {
            name: name.to_string(),
            value,
        }
    }
}

impl DagNode for TestNode {
    fn id(&self) -> &str {
        &self.name
    }

    fn execute(
        &self,
        inputs: HashMap<String, Value>,
        _cancel: CancellationToken,
    ) -> Pin<Box<dyn Future<Output = AppResult<Value>> + Send + '_>> {
        let input_sum: i64 = inputs.values().filter_map(|v| v.as_i64()).sum();
        let result = self.value + input_sum;
        Box::pin(async move { Ok(json!(result)) })
    }
}

#[tokio::test]
async fn create_dag_and_add_nodes() {
    let dag = Dag::new()
        .add_node(TestNode::new("a", 1))
        .add_node(TestNode::new("b", 2));

    let sorted = dag.topological_sort().unwrap();
    assert_eq!(sorted.len(), 2);
    assert!(sorted.contains(&"a".to_string()));
    assert!(sorted.contains(&"b".to_string()));
}

#[tokio::test]
async fn add_edges_between_nodes() {
    let dag = Dag::new()
        .add_node(TestNode::new("a", 1))
        .add_node(TestNode::new("b", 2))
        .add_edge("a", "b")
        .unwrap();

    let sorted = dag.topological_sort().unwrap();
    let pos_a = sorted.iter().position(|n| n == "a").unwrap();
    let pos_b = sorted.iter().position(|n| n == "b").unwrap();
    assert!(pos_a < pos_b);
}

#[tokio::test]
async fn topological_sort_respects_dependencies() {
    let dag = Dag::new()
        .add_node(TestNode::new("a", 1))
        .add_node(TestNode::new("b", 2))
        .add_node(TestNode::new("c", 3))
        .add_edge("a", "b")
        .unwrap()
        .add_edge("b", "c")
        .unwrap();

    let sorted = dag.topological_sort().unwrap();
    assert_eq!(sorted, vec!["a", "b", "c"]);
}

#[tokio::test]
async fn cycle_detection_returns_error() {
    let dag = Dag::new()
        .add_node(TestNode::new("a", 1))
        .add_node(TestNode::new("b", 2))
        .add_edge("a", "b")
        .unwrap()
        .add_edge("b", "a")
        .unwrap();

    let result = dag.topological_sort();
    assert!(result.is_err());
}

#[tokio::test]
async fn execute_linear_dag() {
    let dag = Dag::new()
        .add_node(TestNode::new("a", 10))
        .add_node(TestNode::new("b", 5))
        .add_edge("a", "b")
        .unwrap();

    let cancel = CancellationToken::new();
    let outputs = dag.execute(cancel).await.unwrap();

    assert_eq!(outputs["a"], json!(10));
    assert_eq!(outputs["b"], json!(15)); // 5 + 10 from a
}

#[tokio::test]
async fn execute_diamond_dag() {
    // a -> b, a -> c, b -> d, c -> d
    let dag = Dag::new()
        .add_node(TestNode::new("a", 10))
        .add_node(TestNode::new("b", 1))
        .add_node(TestNode::new("c", 2))
        .add_node(TestNode::new("d", 0))
        .add_edge("a", "b")
        .unwrap()
        .add_edge("a", "c")
        .unwrap()
        .add_edge("b", "d")
        .unwrap()
        .add_edge("c", "d")
        .unwrap();

    let cancel = CancellationToken::new();
    let outputs = dag.execute(cancel).await.unwrap();

    assert_eq!(outputs["a"], json!(10));
    assert_eq!(outputs["b"], json!(11)); // 1 + 10
    assert_eq!(outputs["c"], json!(12)); // 2 + 10
    assert_eq!(outputs["d"], json!(23)); // 0 + 11 + 12
}

#[tokio::test]
async fn add_edge_with_missing_node_fails() {
    let result = Dag::new()
        .add_node(TestNode::new("a", 1))
        .add_edge("a", "nonexistent");

    assert!(result.is_err());
}

#[tokio::test]
async fn execute_independent_nodes() {
    let dag = Dag::new()
        .add_node(TestNode::new("x", 7))
        .add_node(TestNode::new("y", 3));

    let cancel = CancellationToken::new();
    let outputs = dag.execute(cancel).await.unwrap();

    assert_eq!(outputs["x"], json!(7));
    assert_eq!(outputs["y"], json!(3));
}
