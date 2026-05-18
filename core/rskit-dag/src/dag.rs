use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;

use crate::node::DagNode;
use parking_lot::Mutex;
use rskit_errors::{AppError, AppResult, ErrorCode};
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

/// Failure handling strategy for DAG execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum FailurePolicy {
    /// Stop execution immediately when the first node fails.
    FailFast,
    /// Continue executing all nodes, even when upstream dependencies fail.
    Continue,
    /// Skip all downstream dependents of a failed node, but keep independent branches running.
    SkipDependents,
}

struct NodeExecution {
    node_id: String,
    result: AppResult<serde_json::Value>,
}

/// A directed acyclic graph of executable nodes.
///
/// Nodes are executed in topological order with maximum parallelism:
/// all nodes whose dependencies are satisfied run concurrently.
/// Use [`with_max_parallelism`](Dag::with_max_parallelism) to limit
/// how many nodes execute simultaneously.
pub struct Dag {
    nodes: HashMap<String, Arc<dyn DagNode>>,
    /// Forward edges: `from_id` → downstream dependents
    edges: HashMap<String, Vec<String>>,
    /// Reverse edges: `to_id` → upstream dependencies
    reverse_edges: HashMap<String, Vec<String>>,
    /// Limit on concurrent node execution.
    max_parallelism: usize,
    /// Failure handling strategy.
    failure_policy: FailurePolicy,
}

impl Default for Dag {
    fn default() -> Self {
        Self::new()
    }
}

impl Dag {
    /// Create an empty DAG.
    #[must_use]
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            edges: HashMap::new(),
            reverse_edges: HashMap::new(),
            max_parallelism: default_parallelism(),
            failure_policy: FailurePolicy::FailFast,
        }
    }

    /// Set the maximum number of nodes that can execute concurrently.
    #[must_use]
    pub fn with_max_parallelism(mut self, max: usize) -> Self {
        self.max_parallelism = max.max(1);
        self
    }

    /// Set the failure handling policy.
    #[must_use]
    pub fn with_failure_policy(mut self, policy: FailurePolicy) -> Self {
        self.failure_policy = policy;
        self
    }

    /// Add a node to the DAG.
    #[must_use]
    pub fn add_node(mut self, node: impl DagNode) -> Self {
        let id = node.id().to_owned();
        self.nodes.insert(id.clone(), Arc::new(node));
        self.edges.entry(id.clone()).or_default();
        self.reverse_edges.entry(id).or_default();
        self
    }

    /// Add a directed edge from one node to another.
    pub fn add_edge(mut self, from: &str, to: &str) -> AppResult<Self> {
        if !self.nodes.contains_key(from) {
            return Err(AppError::new(
                ErrorCode::InvalidInput,
                format!("DAG node '{from}' not found"),
            ));
        }
        if !self.nodes.contains_key(to) {
            return Err(AppError::new(
                ErrorCode::InvalidInput,
                format!("DAG node '{to}' not found"),
            ));
        }

        if self.path_exists(to, from) {
            return Err(AppError::new(
                ErrorCode::InvalidInput,
                format!("DAG edge '{from}' -> '{to}' would create a cycle"),
            ));
        }

        self.edges
            .entry(from.to_owned())
            .or_default()
            .push(to.to_owned());
        self.reverse_edges
            .entry(to.to_owned())
            .or_default()
            .push(from.to_owned());

        Ok(self)
    }

    /// Topological sort using Kahn's algorithm.
    pub fn topological_sort(&self) -> AppResult<Vec<String>> {
        let mut in_degree: HashMap<String, usize> = HashMap::new();
        for id in self.nodes.keys() {
            in_degree.entry(id.clone()).or_insert(0);
        }
        for (node_id, deps) in &self.reverse_edges {
            *in_degree.entry(node_id.clone()).or_insert(0) = deps.len();
        }

        let mut queue: VecDeque<String> = VecDeque::new();
        for (id, &deg) in &in_degree {
            if deg == 0 {
                queue.push_back(id.clone());
            }
        }

        let mut sorted = Vec::with_capacity(self.nodes.len());
        while let Some(id) = queue.pop_front() {
            sorted.push(id.clone());
            if let Some(dependents) = self.edges.get(&id) {
                for dep in dependents {
                    if let Some(deg) = in_degree.get_mut(dep) {
                        *deg -= 1;
                        if *deg == 0 {
                            queue.push_back(dep.clone());
                        }
                    }
                }
            }
        }

        if sorted.len() != self.nodes.len() {
            return Err(AppError::new(
                ErrorCode::InvalidInput,
                "DAG contains a cycle",
            ));
        }

        Ok(sorted)
    }

    /// Execute all nodes respecting dependency order with maximum parallelism.
    pub async fn execute(
        &self,
        cancel: CancellationToken,
    ) -> AppResult<HashMap<String, serde_json::Value>> {
        self.execute_with_inputs(HashMap::new(), cancel).await
    }

    /// Execute with initial inputs provided to root nodes.
    pub async fn execute_with_inputs(
        &self,
        initial_inputs: HashMap<String, serde_json::Value>,
        cancel: CancellationToken,
    ) -> AppResult<HashMap<String, serde_json::Value>> {
        let _ = self.topological_sort()?;

        let outputs = Arc::new(Mutex::new(HashMap::<String, serde_json::Value>::new()));
        let semaphore = Arc::new(tokio::sync::Semaphore::new(self.max_parallelism));

        let mut remaining_in_degree: HashMap<String, usize> = self
            .nodes
            .keys()
            .map(|id| {
                (
                    id.clone(),
                    self.reverse_edges.get(id).map_or(0, std::vec::Vec::len),
                )
            })
            .collect();

        let mut join_set: JoinSet<NodeExecution> = JoinSet::new();
        let mut pending = HashSet::new();
        let mut completed = HashSet::new();
        let mut failed = HashSet::new();
        let mut skipped = HashSet::new();

        for (id, degree) in &remaining_in_degree {
            if *degree == 0 {
                self.spawn_node(
                    id.clone(),
                    initial_inputs.clone(),
                    cancel.clone(),
                    Arc::clone(&semaphore),
                    &mut join_set,
                    &mut pending,
                )?;
            }
        }

        while let Some(joined) = join_set.join_next().await {
            let execution = joined.map_err(|error| {
                AppError::new(ErrorCode::Internal, format!("DAG task panicked: {error}"))
            })?;
            pending.remove(&execution.node_id);

            match execution.result {
                Ok(value) => {
                    outputs.lock().insert(execution.node_id.clone(), value);
                    completed.insert(execution.node_id.clone());
                    self.schedule_dependents(
                        &execution.node_id,
                        &cancel,
                        &initial_inputs,
                        &outputs,
                        &semaphore,
                        &mut join_set,
                        &mut remaining_in_degree,
                        &mut pending,
                        &completed,
                        &failed,
                        &skipped,
                    )?;
                }
                Err(error) => match self.failure_policy {
                    FailurePolicy::FailFast => return Err(error),
                    FailurePolicy::Continue => {
                        failed.insert(execution.node_id.clone());
                        completed.insert(execution.node_id.clone());
                        self.schedule_dependents(
                            &execution.node_id,
                            &cancel,
                            &initial_inputs,
                            &outputs,
                            &semaphore,
                            &mut join_set,
                            &mut remaining_in_degree,
                            &mut pending,
                            &completed,
                            &failed,
                            &skipped,
                        )?;
                    }
                    FailurePolicy::SkipDependents => {
                        failed.insert(execution.node_id.clone());
                        completed.insert(execution.node_id.clone());
                        mark_skipped_dependents(
                            &self.edges,
                            &execution.node_id,
                            &mut skipped,
                            &pending,
                        );
                    }
                },
            }
        }

        Ok(outputs.lock().clone())
    }

    #[allow(clippy::too_many_arguments)]
    fn schedule_dependents(
        &self,
        finished_id: &str,
        cancel: &CancellationToken,
        initial_inputs: &HashMap<String, serde_json::Value>,
        outputs: &Arc<Mutex<HashMap<String, serde_json::Value>>>,
        semaphore: &Arc<tokio::sync::Semaphore>,
        join_set: &mut JoinSet<NodeExecution>,
        remaining_in_degree: &mut HashMap<String, usize>,
        pending: &mut HashSet<String>,
        completed: &HashSet<String>,
        failed: &HashSet<String>,
        skipped: &HashSet<String>,
    ) -> AppResult<()> {
        if let Some(dependents) = self.edges.get(finished_id) {
            for dependent_id in dependents {
                if completed.contains(dependent_id)
                    || pending.contains(dependent_id)
                    || skipped.contains(dependent_id)
                {
                    continue;
                }
                if let Some(degree) = remaining_in_degree.get_mut(dependent_id) {
                    *degree = degree.saturating_sub(1);
                    if *degree == 0 {
                        let inputs =
                            self.collect_inputs(dependent_id, outputs, failed, initial_inputs);
                        self.spawn_node(
                            dependent_id.clone(),
                            inputs,
                            cancel.clone(),
                            Arc::clone(semaphore),
                            join_set,
                            pending,
                        )?;
                    }
                }
            }
        }
        Ok(())
    }

    fn collect_inputs(
        &self,
        node_id: &str,
        outputs: &Arc<Mutex<HashMap<String, serde_json::Value>>>,
        failed: &HashSet<String>,
        initial_inputs: &HashMap<String, serde_json::Value>,
    ) -> HashMap<String, serde_json::Value> {
        let reverse = self.reverse_edges.get(node_id).cloned().unwrap_or_default();
        if reverse.is_empty() {
            return initial_inputs.clone();
        }

        let output_guard = outputs.lock();
        reverse
            .iter()
            .filter(|dependency| !failed.contains(*dependency))
            .filter_map(|dependency| {
                output_guard
                    .get(dependency)
                    .map(|value| (dependency.clone(), value.clone()))
            })
            .collect()
    }

    fn spawn_node(
        &self,
        node_id: String,
        inputs: HashMap<String, serde_json::Value>,
        cancel: CancellationToken,
        semaphore: Arc<tokio::sync::Semaphore>,
        join_set: &mut JoinSet<NodeExecution>,
        pending: &mut HashSet<String>,
    ) -> AppResult<()> {
        let node = Arc::clone(self.nodes.get(&node_id).ok_or_else(|| {
            AppError::new(
                ErrorCode::Internal,
                format!("DAG node '{node_id}' not found in node map"),
            )
        })?);
        pending.insert(node_id.clone());
        join_set.spawn(async move {
            let permit_result = semaphore
                .acquire()
                .await
                .map_err(|_| AppError::new(ErrorCode::Internal, "DAG semaphore closed"));

            match permit_result {
                Ok(_permit) => {
                    tracing::debug!(node = %node_id, "executing DAG node");
                    NodeExecution {
                        node_id,
                        result: node.execute(inputs, cancel).await,
                    }
                }
                Err(error) => NodeExecution {
                    node_id,
                    result: Err(error),
                },
            }
        });
        Ok(())
    }

    fn path_exists(&self, from: &str, to: &str) -> bool {
        let mut stack = vec![from.to_string()];
        let mut visited = HashSet::new();
        while let Some(node_id) = stack.pop() {
            if node_id == to {
                return true;
            }
            if !visited.insert(node_id.clone()) {
                continue;
            }
            if let Some(children) = self.edges.get(&node_id) {
                stack.extend(children.iter().cloned());
            }
        }
        false
    }
}

fn default_parallelism() -> usize {
    std::thread::available_parallelism().map_or(1, std::num::NonZeroUsize::get)
}

fn mark_skipped_dependents(
    edges: &HashMap<String, Vec<String>>,
    failed_id: &str,
    skipped: &mut HashSet<String>,
    pending: &HashSet<String>,
) {
    let mut stack = edges.get(failed_id).cloned().unwrap_or_default();
    while let Some(node_id) = stack.pop() {
        if pending.contains(&node_id) || !skipped.insert(node_id.clone()) {
            continue;
        }
        if let Some(children) = edges.get(&node_id) {
            stack.extend(children.iter().cloned());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::future::Future;
    use std::pin::Pin;
    use std::sync::atomic::{AtomicUsize, Ordering};

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

    struct CountingNode {
        id: String,
        counter: Arc<AtomicUsize>,
        value: i64,
    }

    impl DagNode for CountingNode {
        fn id(&self) -> &str {
            &self.id
        }

        fn execute(
            &self,
            inputs: HashMap<String, serde_json::Value>,
            _cancel: CancellationToken,
        ) -> Pin<Box<dyn Future<Output = AppResult<serde_json::Value>> + Send + '_>> {
            Box::pin(async move {
                self.counter.fetch_add(1, Ordering::SeqCst);
                let sum = inputs.values().filter_map(|v| v.as_i64()).sum::<i64>() + self.value;
                Ok(serde_json::json!(sum))
            })
        }
    }

    #[tokio::test]
    async fn linear_dag_executes_in_order() {
        let dag = Dag::new()
            .add_node(AddNode {
                id: "a".into(),
                value: 1,
            })
            .add_node(AddNode {
                id: "b".into(),
                value: 2,
            })
            .add_node(AddNode {
                id: "c".into(),
                value: 3,
            });

        let dag = dag.add_edge("a", "b").unwrap().add_edge("b", "c").unwrap();
        let outputs = dag.execute(CancellationToken::new()).await.unwrap();

        assert_eq!(outputs["a"], serde_json::json!(1));
        assert_eq!(outputs["b"], serde_json::json!(3));
        assert_eq!(outputs["c"], serde_json::json!(6));
    }

    #[tokio::test]
    async fn diamond_dag_merges_inputs() {
        let dag = Dag::new()
            .add_node(AddNode {
                id: "a".into(),
                value: 10,
            })
            .add_node(AddNode {
                id: "b".into(),
                value: 1,
            })
            .add_node(AddNode {
                id: "c".into(),
                value: 2,
            })
            .add_node(AddNode {
                id: "d".into(),
                value: 0,
            });

        let dag = dag
            .add_edge("a", "b")
            .unwrap()
            .add_edge("a", "c")
            .unwrap()
            .add_edge("b", "d")
            .unwrap()
            .add_edge("c", "d")
            .unwrap();

        let outputs = dag.execute(CancellationToken::new()).await.unwrap();

        assert_eq!(outputs["a"], serde_json::json!(10));
        assert_eq!(outputs["b"], serde_json::json!(11));
        assert_eq!(outputs["c"], serde_json::json!(12));
        assert_eq!(outputs["d"], serde_json::json!(23));
    }

    #[tokio::test]
    async fn fail_fast_returns_first_error() {
        let dag = Dag::new()
            .add_node(FailNode { id: "a".into() })
            .add_node(AddNode {
                id: "b".into(),
                value: 1,
            })
            .add_edge("a", "b")
            .unwrap();

        let result = dag.execute(CancellationToken::new()).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn continue_runs_dependents_with_partial_inputs() {
        let dependent_runs = Arc::new(AtomicUsize::new(0));
        let independent_runs = Arc::new(AtomicUsize::new(0));
        let dag = Dag::new()
            .with_failure_policy(FailurePolicy::Continue)
            .add_node(FailNode { id: "a".into() })
            .add_node(CountingNode {
                id: "b".into(),
                counter: dependent_runs.clone(),
                value: 5,
            })
            .add_node(CountingNode {
                id: "c".into(),
                counter: independent_runs.clone(),
                value: 9,
            });

        let dag = dag.add_edge("a", "b").unwrap();
        let outputs = dag.execute(CancellationToken::new()).await.unwrap();

        assert_eq!(dependent_runs.load(Ordering::SeqCst), 1);
        assert_eq!(independent_runs.load(Ordering::SeqCst), 1);
        assert_eq!(outputs["b"], serde_json::json!(5));
        assert_eq!(outputs["c"], serde_json::json!(9));
        assert!(!outputs.contains_key("a"));
    }

    #[tokio::test]
    async fn skip_dependents_skips_failed_branch_only() {
        let dependent_runs = Arc::new(AtomicUsize::new(0));
        let independent_runs = Arc::new(AtomicUsize::new(0));
        let dag = Dag::new()
            .with_failure_policy(FailurePolicy::SkipDependents)
            .add_node(FailNode { id: "a".into() })
            .add_node(CountingNode {
                id: "b".into(),
                counter: dependent_runs.clone(),
                value: 5,
            })
            .add_node(CountingNode {
                id: "c".into(),
                counter: independent_runs.clone(),
                value: 9,
            });

        let dag = dag.add_edge("a", "b").unwrap();
        let outputs = dag.execute(CancellationToken::new()).await.unwrap();

        assert_eq!(dependent_runs.load(Ordering::SeqCst), 0);
        assert_eq!(independent_runs.load(Ordering::SeqCst), 1);
        assert!(!outputs.contains_key("b"));
        assert_eq!(outputs["c"], serde_json::json!(9));
    }

    #[test]
    fn cycle_detection() {
        let dag = Dag::new()
            .add_node(AddNode {
                id: "a".into(),
                value: 0,
            })
            .add_node(AddNode {
                id: "b".into(),
                value: 0,
            });

        let result = dag.add_edge("a", "b").unwrap().add_edge("b", "a");
        assert!(result.is_err());
    }
}
