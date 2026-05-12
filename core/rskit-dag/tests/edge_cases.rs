use rskit_dag::{Dag, DagNode};
use rskit_errors::{AppError, AppResult, ErrorCode};
use serde_json::{Value, json};
use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;

// ---------------------------------------------------------------------------
// Helper node types
// ---------------------------------------------------------------------------

/// Sums all input values and adds its own value.
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

/// Always returns an error.
struct FailNode {
    name: String,
}

impl FailNode {
    fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
        }
    }
}

impl DagNode for FailNode {
    fn id(&self) -> &str {
        &self.name
    }

    fn execute(
        &self,
        _inputs: HashMap<String, Value>,
        _cancel: CancellationToken,
    ) -> Pin<Box<dyn Future<Output = AppResult<Value>> + Send + '_>> {
        Box::pin(async { Err(AppError::new(ErrorCode::Internal, "intentional failure")) })
    }
}

/// Sleeps for the specified duration, then returns its value.
struct SlowNode {
    name: String,
    value: i64,
    delay: Duration,
}

impl SlowNode {
    fn new(name: &str, value: i64, delay: Duration) -> Self {
        Self {
            name: name.to_string(),
            value,
            delay,
        }
    }
}

impl DagNode for SlowNode {
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
        let delay = self.delay;
        Box::pin(async move {
            tokio::time::sleep(delay).await;
            Ok(json!(result))
        })
    }
}

/// Checks the cancellation token; returns an error if cancelled.
struct CancelAwareNode {
    name: String,
    value: i64,
    work_duration: Duration,
}

impl CancelAwareNode {
    fn new(name: &str, value: i64, work_duration: Duration) -> Self {
        Self {
            name: name.to_string(),
            value,
            work_duration,
        }
    }
}

impl DagNode for CancelAwareNode {
    fn id(&self) -> &str {
        &self.name
    }

    fn execute(
        &self,
        inputs: HashMap<String, Value>,
        cancel: CancellationToken,
    ) -> Pin<Box<dyn Future<Output = AppResult<Value>> + Send + '_>> {
        let input_sum: i64 = inputs.values().filter_map(|v| v.as_i64()).sum();
        let result = self.value + input_sum;
        let dur = self.work_duration;
        Box::pin(async move {
            tokio::select! {
                _ = cancel.cancelled() => {
                    Err(AppError::new(ErrorCode::Internal, "cancelled"))
                }
                _ = tokio::time::sleep(dur) => {
                    Ok(json!(result))
                }
            }
        })
    }
}

// ===========================================================================
// Complex Topologies
// ===========================================================================

#[tokio::test]
async fn test_deep_chain_20_levels() {
    // Linear chain: n0 → n1 → … → n19, each adds 1
    let mut dag = Dag::new();
    for i in 0..20 {
        dag = dag.add_node(TestNode::new(&format!("n{i}"), 1));
    }
    for i in 0..19 {
        dag = dag
            .add_edge(&format!("n{i}"), &format!("n{}", i + 1))
            .unwrap();
    }

    let outputs = dag.execute(CancellationToken::new()).await.unwrap();
    // n0=1, n1=1+1=2, …, n19=20
    for i in 0..20u64 {
        assert_eq!(
            outputs[&format!("n{i}")],
            json!(i as i64 + 1),
            "node n{i} mismatch"
        );
    }
}

#[tokio::test]
async fn test_wide_50_independent_nodes() {
    let mut dag = Dag::new();
    for i in 0..50 {
        dag = dag.add_node(TestNode::new(&format!("n{i}"), i));
    }

    let outputs = dag.execute(CancellationToken::new()).await.unwrap();
    assert_eq!(outputs.len(), 50);
    for i in 0..50 {
        assert_eq!(outputs[&format!("n{i}")], json!(i));
    }
}

#[tokio::test]
async fn test_double_diamond() {
    // a→(b,c)→d→(e,f)→g
    let dag = Dag::new()
        .add_node(TestNode::new("a", 1))
        .add_node(TestNode::new("b", 2))
        .add_node(TestNode::new("c", 3))
        .add_node(TestNode::new("d", 0))
        .add_node(TestNode::new("e", 4))
        .add_node(TestNode::new("f", 5))
        .add_node(TestNode::new("g", 0))
        .add_edge("a", "b")
        .unwrap()
        .add_edge("a", "c")
        .unwrap()
        .add_edge("b", "d")
        .unwrap()
        .add_edge("c", "d")
        .unwrap()
        .add_edge("d", "e")
        .unwrap()
        .add_edge("d", "f")
        .unwrap()
        .add_edge("e", "g")
        .unwrap()
        .add_edge("f", "g")
        .unwrap();

    let outputs = dag.execute(CancellationToken::new()).await.unwrap();
    // a=1, b=1+2=3, c=1+3=4, d=3+4+0=7, e=7+4=11, f=7+5=12, g=11+12=23
    assert_eq!(outputs["a"], json!(1));
    assert_eq!(outputs["b"], json!(3));
    assert_eq!(outputs["c"], json!(4));
    assert_eq!(outputs["d"], json!(7));
    assert_eq!(outputs["e"], json!(11));
    assert_eq!(outputs["f"], json!(12));
    assert_eq!(outputs["g"], json!(23));
}

#[tokio::test]
async fn test_multi_fan_out() {
    // root → c0..c9
    let mut dag = Dag::new().add_node(TestNode::new("root", 10));
    for i in 0..10 {
        dag = dag.add_node(TestNode::new(&format!("c{i}"), i));
    }
    for i in 0..10 {
        dag = dag.add_edge("root", &format!("c{i}")).unwrap();
    }

    let outputs = dag.execute(CancellationToken::new()).await.unwrap();
    assert_eq!(outputs["root"], json!(10));
    for i in 0..10 {
        // each child = own value + root's 10
        assert_eq!(outputs[&format!("c{i}")], json!(i + 10));
    }
}

#[tokio::test]
async fn test_multi_fan_in() {
    // r0..r9 → sink
    let mut dag = Dag::new();
    for i in 0..10 {
        dag = dag.add_node(TestNode::new(&format!("r{i}"), i));
    }
    dag = dag.add_node(TestNode::new("sink", 0));
    for i in 0..10 {
        dag = dag.add_edge(&format!("r{i}"), "sink").unwrap();
    }

    let outputs = dag.execute(CancellationToken::new()).await.unwrap();
    // sink = 0 + sum(0..10) = 45
    assert_eq!(outputs["sink"], json!(45));
}

#[tokio::test]
async fn test_deep_narrow_chain() {
    // 10-node chain, each adds 1 → final = 10
    let mut dag = Dag::new();
    for i in 0..10 {
        dag = dag.add_node(TestNode::new(&format!("s{i}"), 1));
    }
    for i in 0..9 {
        dag = dag
            .add_edge(&format!("s{i}"), &format!("s{}", i + 1))
            .unwrap();
    }

    let outputs = dag.execute(CancellationToken::new()).await.unwrap();
    assert_eq!(outputs["s9"], json!(10));
}

#[tokio::test]
async fn test_complex_mixed_topology() {
    // W-shape: a→b→c, a→d, c→e, d→e
    let dag = Dag::new()
        .add_node(TestNode::new("a", 1))
        .add_node(TestNode::new("b", 2))
        .add_node(TestNode::new("c", 3))
        .add_node(TestNode::new("d", 4))
        .add_node(TestNode::new("e", 0))
        .add_edge("a", "b")
        .unwrap()
        .add_edge("b", "c")
        .unwrap()
        .add_edge("a", "d")
        .unwrap()
        .add_edge("c", "e")
        .unwrap()
        .add_edge("d", "e")
        .unwrap();

    let outputs = dag.execute(CancellationToken::new()).await.unwrap();
    // a=1, b=1+2=3, c=3+3=6, d=1+4=5, e=6+5+0=11
    assert_eq!(outputs["a"], json!(1));
    assert_eq!(outputs["b"], json!(3));
    assert_eq!(outputs["c"], json!(6));
    assert_eq!(outputs["d"], json!(5));
    assert_eq!(outputs["e"], json!(11));
}

// ===========================================================================
// Failure Scenarios
// ===========================================================================

#[tokio::test]
async fn test_node_failure_propagates_error() {
    let dag = Dag::new().add_node(FailNode::new("bad"));
    let result = dag.execute(CancellationToken::new()).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_root_failure_prevents_downstream() {
    let dag = Dag::new()
        .add_node(FailNode::new("root"))
        .add_node(TestNode::new("child", 1))
        .add_edge("root", "child")
        .unwrap();

    let result = dag.execute(CancellationToken::new()).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_failure_in_middle_of_chain() {
    // a → b(fail) → c
    let dag = Dag::new()
        .add_node(TestNode::new("a", 1))
        .add_node(FailNode::new("b"))
        .add_node(TestNode::new("c", 3))
        .add_edge("a", "b")
        .unwrap()
        .add_edge("b", "c")
        .unwrap();

    let result = dag.execute(CancellationToken::new()).await;
    assert!(result.is_err());
}

#[tokio::test]
async fn test_independent_node_failure_isolates() {
    // a(fail) and b are independent — execute returns first error encountered
    let dag = Dag::new()
        .add_node(FailNode::new("a"))
        .add_node(TestNode::new("b", 42));

    let result = dag.execute(CancellationToken::new()).await;
    // The DAG returns an error because at least one node failed
    assert!(result.is_err());
}

#[tokio::test]
async fn test_error_node_returns_clean_app_error() {
    // Verify the error from a failing node is an AppError (not a panic)
    let dag = Dag::new().add_node(FailNode::new("err_node"));
    let result = dag.execute(CancellationToken::new()).await;
    let err = result.unwrap_err();
    let msg = err.to_string();
    assert!(
        msg.contains("intentional failure"),
        "unexpected error message: {msg}"
    );
}

// ===========================================================================
// Timeout / Cancellation Handling
// ===========================================================================

#[tokio::test]
async fn test_cancellation_stops_execution() {
    let cancel = CancellationToken::new();
    let dag = Dag::new().add_node(CancelAwareNode::new("slow", 1, Duration::from_secs(10)));

    let cancel_clone = cancel.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(50)).await;
        cancel_clone.cancel();
    });

    let result = dag.execute(cancel).await;
    assert!(result.is_err(), "expected cancellation error");
}

#[tokio::test]
async fn test_slow_node_with_cancel() {
    let cancel = CancellationToken::new();
    let dag = Dag::new().add_node(CancelAwareNode::new("work", 99, Duration::from_secs(30)));

    let cancel_clone = cancel.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(20)).await;
        cancel_clone.cancel();
    });

    let start = Instant::now();
    let result = dag.execute(cancel).await;
    let elapsed = start.elapsed();

    assert!(result.is_err());
    // Should finish well before 30 seconds
    assert!(
        elapsed < Duration::from_secs(2),
        "took too long: {elapsed:?}"
    );
}

#[tokio::test]
async fn test_all_nodes_receive_cancel_token() {
    let cancel = CancellationToken::new();

    // Two independent CancelAwareNodes — both should see cancellation
    let dag = Dag::new()
        .add_node(CancelAwareNode::new("a", 1, Duration::from_secs(10)))
        .add_node(CancelAwareNode::new("b", 2, Duration::from_secs(10)));

    let cancel_clone = cancel.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(30)).await;
        cancel_clone.cancel();
    });

    let start = Instant::now();
    let result = dag.execute(cancel).await;
    let elapsed = start.elapsed();

    assert!(result.is_err());
    assert!(
        elapsed < Duration::from_secs(2),
        "both nodes should have been cancelled quickly, took {elapsed:?}"
    );
}

// ===========================================================================
// Cycle Detection Edge Cases
// ===========================================================================

#[tokio::test]
async fn test_self_cycle() {
    let dag = Dag::new()
        .add_node(TestNode::new("a", 1))
        .add_edge("a", "a")
        .unwrap();

    let result = dag.topological_sort();
    assert!(result.is_err(), "self-cycle should be detected");
}

#[tokio::test]
async fn test_three_node_cycle() {
    let dag = Dag::new()
        .add_node(TestNode::new("a", 1))
        .add_node(TestNode::new("b", 2))
        .add_node(TestNode::new("c", 3))
        .add_edge("a", "b")
        .unwrap()
        .add_edge("b", "c")
        .unwrap()
        .add_edge("c", "a")
        .unwrap();

    assert!(dag.topological_sort().is_err());
    assert!(dag.execute(CancellationToken::new()).await.is_err());
}

#[tokio::test]
async fn test_cycle_in_subgraph() {
    // a → b → c → b  (b↔c cycle; a is root)
    let dag = Dag::new()
        .add_node(TestNode::new("a", 1))
        .add_node(TestNode::new("b", 2))
        .add_node(TestNode::new("c", 3))
        .add_edge("a", "b")
        .unwrap()
        .add_edge("b", "c")
        .unwrap()
        .add_edge("c", "b")
        .unwrap();

    assert!(dag.topological_sort().is_err());
}

#[tokio::test]
async fn test_large_ring_cycle() {
    // n0 → n1 → … → n9 → n0
    let mut dag = Dag::new();
    for i in 0..10 {
        dag = dag.add_node(TestNode::new(&format!("n{i}"), 1));
    }
    for i in 0..10 {
        dag = dag
            .add_edge(&format!("n{i}"), &format!("n{}", (i + 1) % 10))
            .unwrap();
    }

    assert!(dag.topological_sort().is_err());
}

// ===========================================================================
// Concurrent Execution (timing)
// ===========================================================================

#[tokio::test]
async fn test_concurrent_independent_nodes_timing() {
    // 5 independent 100ms nodes — parallel should finish in ~100ms, not 500ms
    let mut dag = Dag::new();
    for i in 0..5 {
        dag = dag.add_node(SlowNode::new(
            &format!("s{i}"),
            i,
            Duration::from_millis(100),
        ));
    }

    let start = Instant::now();
    let outputs = dag.execute(CancellationToken::new()).await.unwrap();
    let elapsed = start.elapsed();

    assert_eq!(outputs.len(), 5);
    assert!(
        elapsed < Duration::from_millis(400),
        "expected parallel execution, took {elapsed:?}"
    );
}

#[tokio::test]
async fn test_diamond_parallelism() {
    // a(fast) → (b 100ms, c 100ms) → d(fast)
    // Total should be ~200ms (a + max(b,c) + d), not 300ms (sequential)
    let dag = Dag::new()
        .add_node(SlowNode::new("a", 1, Duration::from_millis(10)))
        .add_node(SlowNode::new("b", 2, Duration::from_millis(100)))
        .add_node(SlowNode::new("c", 3, Duration::from_millis(100)))
        .add_node(SlowNode::new("d", 0, Duration::from_millis(10)))
        .add_edge("a", "b")
        .unwrap()
        .add_edge("a", "c")
        .unwrap()
        .add_edge("b", "d")
        .unwrap()
        .add_edge("c", "d")
        .unwrap();

    let start = Instant::now();
    let outputs = dag.execute(CancellationToken::new()).await.unwrap();
    let elapsed = start.elapsed();

    // a=1, b=1+2=3, c=1+3=4, d=3+4+0=7
    assert_eq!(outputs["d"], json!(7));
    assert!(
        elapsed < Duration::from_millis(350),
        "b and c should run in parallel, took {elapsed:?}"
    );
}

#[tokio::test]
async fn test_large_parallel_execution() {
    // 20 independent 50ms nodes — should complete in ~50ms, not 1000ms
    let mut dag = Dag::new();
    for i in 0..20 {
        dag = dag.add_node(SlowNode::new(
            &format!("p{i}"),
            1,
            Duration::from_millis(50),
        ));
    }

    let start = Instant::now();
    let outputs = dag.execute(CancellationToken::new()).await.unwrap();
    let elapsed = start.elapsed();

    assert_eq!(outputs.len(), 20);
    assert!(
        elapsed < Duration::from_millis(300),
        "expected parallel execution, took {elapsed:?}"
    );
}

// ===========================================================================
// Large Node Graphs
// ===========================================================================

#[tokio::test]
async fn test_100_node_linear_chain() {
    let mut dag = Dag::new();
    for i in 0..100 {
        dag = dag.add_node(TestNode::new(&format!("n{i}"), 1));
    }
    for i in 0..99 {
        dag = dag
            .add_edge(&format!("n{i}"), &format!("n{}", i + 1))
            .unwrap();
    }

    let outputs = dag.execute(CancellationToken::new()).await.unwrap();
    assert_eq!(outputs.len(), 100);
    assert_eq!(outputs["n99"], json!(100));
}

#[tokio::test]
async fn test_100_independent_nodes() {
    let mut dag = Dag::new();
    for i in 0..100 {
        dag = dag.add_node(TestNode::new(&format!("n{i}"), i));
    }

    let outputs = dag.execute(CancellationToken::new()).await.unwrap();
    assert_eq!(outputs.len(), 100);
    for i in 0..100 {
        assert_eq!(outputs[&format!("n{i}")], json!(i));
    }
}

#[tokio::test]
async fn test_50_node_diamond() {
    // root → m0..m47 → sink
    let mut dag = Dag::new().add_node(TestNode::new("root", 1));
    for i in 0..48 {
        dag = dag.add_node(TestNode::new(&format!("m{i}"), 1));
    }
    dag = dag.add_node(TestNode::new("sink", 0));

    for i in 0..48 {
        dag = dag.add_edge("root", &format!("m{i}")).unwrap();
        dag = dag.add_edge(&format!("m{i}"), "sink").unwrap();
    }

    let outputs = dag.execute(CancellationToken::new()).await.unwrap();
    assert_eq!(outputs.len(), 50);
    // root=1, each middle = 1+1 = 2, sink = 48*2 + 0 = 96
    assert_eq!(outputs["root"], json!(1));
    assert_eq!(outputs["m0"], json!(2));
    assert_eq!(outputs["sink"], json!(96));
}
