use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::node::DagNode;
use crate::result::{DagResult, NodeResult, NodeStatus, OnError};
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

impl FailurePolicy {
    /// Default per-node [`OnError`] behavior implied by this DAG-level policy.
    const fn default_on_error(self) -> OnError {
        match self {
            Self::FailFast => OnError::Fail,
            Self::Continue => OnError::Continue,
            Self::SkipDependents => OnError::Skip,
        }
    }
}

/// Terminal status derived from a node's execution error.
const fn status_from_error(error: &AppError) -> NodeStatus {
    match error.code() {
        ErrorCode::Timeout => NodeStatus::TimedOut,
        ErrorCode::Cancelled => NodeStatus::Canceled,
        _ => NodeStatus::Failed,
    }
}

struct NodeExecution {
    node_id: String,
    result: AppResult<serde_json::Value>,
    duration: Duration,
}

/// Mutable scheduler state threaded through a single DAG execution run.
struct ExecutionRun {
    initial_inputs: HashMap<String, serde_json::Value>,
    outputs: Arc<Mutex<HashMap<String, serde_json::Value>>>,
    semaphore: Arc<tokio::sync::Semaphore>,
    cancel: CancellationToken,
    join_set: JoinSet<NodeExecution>,
    remaining_in_degree: HashMap<String, usize>,
    pending: HashSet<String>,
    completed: HashSet<String>,
    failed: HashSet<String>,
    skipped: HashSet<String>,
    node_results: HashMap<String, NodeResult>,
    first_error: Option<AppError>,
    aborted: bool,
    started: Instant,
}

/// A directed acyclic graph of executable nodes.
///
/// Nodes are executed in topological order with maximum parallelism:
/// all nodes whose dependencies are satisfied run concurrently.
/// Use [`with_max_parallelism`](Dag::with_max_parallelism) to limit how many nodes execute simultaneously.
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
    /// Per-node error-handling overrides above [`failure_policy`](Self::failure_policy).
    node_on_error: HashMap<String, OnError>,
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
            node_on_error: HashMap::new(),
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
    pub const fn with_failure_policy(mut self, policy: FailurePolicy) -> Self {
        self.failure_policy = policy;
        self
    }

    /// Override how a specific node's failure affects the rest of the run.
    ///
    /// The override takes precedence over the DAG-level
    /// [`FailurePolicy`](Self::with_failure_policy) for that node only.
    #[must_use]
    pub fn with_on_error(mut self, node_id: impl Into<String>, on_error: OnError) -> Self {
        self.node_on_error.insert(node_id.into(), on_error);
        self
    }

    /// Resolve the effective [`OnError`] behavior for a node.
    fn effective_on_error(&self, node_id: &str) -> OnError {
        self.node_on_error
            .get(node_id)
            .copied()
            .unwrap_or_else(|| self.failure_policy.default_on_error())
    }

    /// Reject `on_error` overrides that reference nodes the DAG does not contain, mirroring
    /// [`add_edge`](Self::add_edge)'s validation so a misspelled node id cannot silently fall back
    /// to the DAG-wide policy.
    fn validate_on_error_overrides(&self) -> AppResult<()> {
        for node_id in self.node_on_error.keys() {
            if !self.nodes.contains_key(node_id) {
                return Err(AppError::new(
                    ErrorCode::InvalidInput,
                    format!("DAG on-error override references unknown node '{node_id}'"),
                ));
            }
        }
        Ok(())
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
    ///
    /// Returns the outputs of every completed node. Node failures are governed by the
    /// [`FailurePolicy`](Self::with_failure_policy) and per-node
    /// [`OnError`](Self::with_on_error) overrides; a `Fail` outcome surfaces the node's error.
    /// Use [`run`](Self::run) to obtain a full [`DagResult`] instead.
    pub async fn execute(
        &self,
        cancel: CancellationToken,
    ) -> AppResult<HashMap<String, serde_json::Value>> {
        self.execute_with_inputs(HashMap::new(), cancel).await
    }

    /// Execute with initial inputs provided to root nodes, returning completed-node outputs.
    pub async fn execute_with_inputs(
        &self,
        initial_inputs: HashMap<String, serde_json::Value>,
        cancel: CancellationToken,
    ) -> AppResult<HashMap<String, serde_json::Value>> {
        let mut run = self.run_engine(initial_inputs, cancel).await?;
        if run.aborted
            && let Some(error) = run.first_error.take()
        {
            return Err(error);
        }
        Ok(run.outputs.lock().clone())
    }

    /// Execute all nodes and collect a full [`DagResult`] with per-node status, duration, and error.
    pub async fn run(&self, cancel: CancellationToken) -> AppResult<DagResult> {
        self.run_with_inputs(HashMap::new(), cancel).await
    }

    /// Execute with initial inputs and collect a full [`DagResult`].
    ///
    /// Unlike [`execute_with_inputs`](Self::execute_with_inputs), node failures never surface as an
    /// `Err`: they are recorded in the returned result. `Err` is reserved for structural failures
    /// such as a cycle or a panicked task.
    pub async fn run_with_inputs(
        &self,
        initial_inputs: HashMap<String, serde_json::Value>,
        cancel: CancellationToken,
    ) -> AppResult<DagResult> {
        let run = self.run_engine(initial_inputs, cancel).await?;
        Ok(self.build_result(run))
    }

    /// Drive the scheduler to completion, recording per-node results in the returned run state.
    async fn run_engine(
        &self,
        initial_inputs: HashMap<String, serde_json::Value>,
        cancel: CancellationToken,
    ) -> AppResult<ExecutionRun> {
        let _ = self.topological_sort()?;
        self.validate_on_error_overrides()?;

        let remaining_in_degree: HashMap<String, usize> = self
            .nodes
            .keys()
            .map(|id| {
                (
                    id.clone(),
                    self.reverse_edges.get(id).map_or(0, std::vec::Vec::len),
                )
            })
            .collect();

        let mut run = ExecutionRun {
            initial_inputs,
            outputs: Arc::new(Mutex::new(HashMap::new())),
            semaphore: Arc::new(tokio::sync::Semaphore::new(self.max_parallelism)),
            // Derive an internal child token: the caller's cancellation still propagates down
            // (child inherits parent), but a fail-fast abort inside this run cancels only the
            // child, never the caller-owned token that may be shared with the rest of the app.
            cancel: cancel.child_token(),
            join_set: JoinSet::new(),
            remaining_in_degree,
            pending: HashSet::new(),
            completed: HashSet::new(),
            failed: HashSet::new(),
            skipped: HashSet::new(),
            node_results: HashMap::new(),
            first_error: None,
            aborted: false,
            started: Instant::now(),
        };

        let roots: Vec<String> = run
            .remaining_in_degree
            .iter()
            .filter(|(_, degree)| **degree == 0)
            .map(|(id, _)| id.clone())
            .collect();
        for id in roots {
            let inputs = run.initial_inputs.clone();
            self.spawn_node(id, inputs, &mut run)?;
        }

        while let Some(joined) = run.join_set.join_next().await {
            let execution = match joined {
                Ok(execution) => execution,
                // A fail-fast abort drops outstanding tasks; their nodes are recorded as
                // `Canceled` in `build_result`. Only a genuine panic is an internal error.
                Err(join_error) if join_error.is_cancelled() => continue,
                Err(join_error) => {
                    return Err(AppError::new(
                        ErrorCode::Internal,
                        format!("DAG task panicked: {join_error}"),
                    ));
                }
            };
            self.record_execution(execution, &mut run)?;
        }

        Ok(run)
    }

    /// Record one node's outcome and apply its failure policy to the rest of the run.
    fn record_execution(&self, execution: NodeExecution, run: &mut ExecutionRun) -> AppResult<()> {
        run.pending.remove(&execution.node_id);
        let duration_ms = duration_millis(execution.duration);

        match execution.result {
            Ok(value) => {
                run.node_results.insert(
                    execution.node_id.clone(),
                    NodeResult {
                        name: execution.node_id.clone(),
                        status: NodeStatus::Completed,
                        duration_ms,
                        output: Some(value.clone()),
                        error: None,
                    },
                );
                run.outputs.lock().insert(execution.node_id.clone(), value);
                run.completed.insert(execution.node_id.clone());
                self.schedule_dependents(&execution.node_id, run)?;
            }
            Err(error) => {
                let status = status_from_error(&error);
                run.node_results.insert(
                    execution.node_id.clone(),
                    NodeResult {
                        name: execution.node_id.clone(),
                        status,
                        duration_ms,
                        output: None,
                        error: Some(error.to_string()),
                    },
                );
                run.failed.insert(execution.node_id.clone());
                run.completed.insert(execution.node_id.clone());

                match self.effective_on_error(&execution.node_id) {
                    OnError::Fail => {
                        // Only a fail-fast node's error aborts the run, so this is the error
                        // `execute` surfaces — a non-aborting `Continue`/`Skip` failure never
                        // masks it.
                        if run.first_error.is_none() {
                            run.first_error = Some(error);
                        }
                        run.aborted = true;
                        run.cancel.cancel();
                        // Stop waiting on independent in-flight nodes that may ignore the
                        // cooperative token; drained joins surface as cancellations above.
                        run.join_set.abort_all();
                    }
                    OnError::Continue => {
                        self.schedule_dependents(&execution.node_id, run)?;
                    }
                    OnError::Skip => {
                        mark_skipped_dependents(
                            &self.edges,
                            &execution.node_id,
                            &mut run.skipped,
                            &run.pending,
                        );
                    }
                }
            }
        }
        Ok(())
    }

    /// Assemble a [`DagResult`], assigning terminal statuses to nodes that never produced one.
    fn build_result(&self, run: ExecutionRun) -> DagResult {
        let ExecutionRun {
            mut node_results,
            skipped,
            aborted,
            started,
            ..
        } = run;
        let mut nodes = BTreeMap::new();
        for id in self.nodes.keys() {
            let result = node_results.remove(id).unwrap_or_else(|| {
                let status = if aborted && !skipped.contains(id) {
                    NodeStatus::Canceled
                } else {
                    NodeStatus::Skipped
                };
                NodeResult {
                    name: id.clone(),
                    status,
                    duration_ms: 0,
                    output: None,
                    error: None,
                }
            });
            nodes.insert(id.clone(), result);
        }
        // Total elapsed wall-clock time, not the sum of per-node times, which overcounts when
        // nodes run concurrently.
        let duration_ms = duration_millis(started.elapsed());
        DagResult { nodes, duration_ms }
    }

    fn schedule_dependents(&self, finished_id: &str, run: &mut ExecutionRun) -> AppResult<()> {
        let Some(dependents) = self.edges.get(finished_id) else {
            return Ok(());
        };
        for dependent_id in dependents {
            if run.completed.contains(dependent_id)
                || run.pending.contains(dependent_id)
                || run.skipped.contains(dependent_id)
            {
                continue;
            }
            let ready = match run.remaining_in_degree.get_mut(dependent_id) {
                Some(degree) => {
                    let next = degree.checked_sub(1).ok_or_else(|| {
                        AppError::new(
                            ErrorCode::Internal,
                            format!("DAG in-degree underflow for node '{dependent_id}'"),
                        )
                    })?;
                    *degree = next;
                    next == 0
                }
                None => false,
            };
            if ready {
                let inputs = self.collect_inputs(
                    dependent_id,
                    &run.outputs,
                    &run.failed,
                    &run.initial_inputs,
                );
                self.spawn_node(dependent_id.clone(), inputs, run)?;
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
        run: &mut ExecutionRun,
    ) -> AppResult<()> {
        let node = Arc::clone(self.nodes.get(&node_id).ok_or_else(|| {
            AppError::new(
                ErrorCode::Internal,
                format!("DAG node '{node_id}' not found in node map"),
            )
        })?);
        run.pending.insert(node_id.clone());
        let cancel = run.cancel.clone();
        let semaphore = Arc::clone(&run.semaphore);
        run.join_set.spawn(async move {
            let permit_result = semaphore
                .acquire()
                .await
                .map_err(|_| AppError::new(ErrorCode::Internal, "DAG semaphore closed"));

            match permit_result {
                Ok(_permit) => {
                    tracing::debug!(node = %node_id, "executing DAG node");
                    let started = Instant::now();
                    let result = node.execute(inputs, cancel).await;
                    NodeExecution {
                        node_id,
                        result,
                        duration: started.elapsed(),
                    }
                }
                Err(error) => NodeExecution {
                    node_id,
                    result: Err(error),
                    duration: Duration::ZERO,
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

/// Convert an execution [`Duration`] to whole milliseconds, saturating at [`u64::MAX`].
fn duration_millis(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
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
                let sum: i64 = inputs
                    .values()
                    .filter_map(serde_json::Value::as_i64)
                    .sum::<i64>()
                    + self.value;
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
                let sum = inputs
                    .values()
                    .filter_map(serde_json::Value::as_i64)
                    .sum::<i64>()
                    + self.value;
                Ok(serde_json::json!(sum))
            })
        }
    }

    struct MessageFailNode {
        id: String,
        message: String,
    }

    impl DagNode for MessageFailNode {
        fn id(&self) -> &str {
            &self.id
        }

        fn execute(
            &self,
            _inputs: HashMap<String, serde_json::Value>,
            _cancel: CancellationToken,
        ) -> Pin<Box<dyn Future<Output = AppResult<serde_json::Value>> + Send + '_>> {
            let message = self.message.clone();
            Box::pin(async move { Err(AppError::new(ErrorCode::Internal, message)) })
        }
    }

    /// A node that sleeps for a fixed duration and ignores the cancellation token, used to prove
    /// fail-fast aborts outstanding work and that run duration is measured as wall-clock time.
    struct SleepNode {
        id: String,
        duration: Duration,
    }

    impl DagNode for SleepNode {
        fn id(&self) -> &str {
            &self.id
        }

        fn execute(
            &self,
            _inputs: HashMap<String, serde_json::Value>,
            _cancel: CancellationToken,
        ) -> Pin<Box<dyn Future<Output = AppResult<serde_json::Value>> + Send + '_>> {
            let duration = self.duration;
            Box::pin(async move {
                tokio::time::sleep(duration).await;
                Ok(serde_json::json!("done"))
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
    async fn fail_fast_does_not_cancel_caller_token() {
        let dag = Dag::new()
            .add_node(FailNode { id: "a".into() })
            .add_node(AddNode {
                id: "b".into(),
                value: 1,
            })
            .add_edge("a", "b")
            .unwrap();

        // A caller-owned token, potentially shared with the rest of the application.
        let caller = CancellationToken::new();
        let result = dag.execute(caller.clone()).await;
        assert!(result.is_err());
        assert!(
            !caller.is_cancelled(),
            "fail-fast abort must not cancel the caller-owned token"
        );
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

    #[tokio::test]
    async fn run_reports_per_node_status() {
        let dag = Dag::new()
            .add_node(AddNode {
                id: "a".into(),
                value: 1,
            })
            .add_node(FailNode { id: "b".into() })
            .add_node(AddNode {
                id: "c".into(),
                value: 1,
            })
            .add_edge("b", "c")
            .unwrap()
            .with_failure_policy(FailurePolicy::SkipDependents);

        let result = dag.run(CancellationToken::new()).await.unwrap();

        assert_eq!(result.status("a"), Some(NodeStatus::Completed));
        assert_eq!(result.status("b"), Some(NodeStatus::Failed));
        assert_eq!(result.status("c"), Some(NodeStatus::Skipped));
        assert!(!result.is_success());
        assert_eq!(
            result.nodes.get("b").and_then(|node| node.error.as_deref()),
            Some("INTERNAL_ERROR: node failed")
        );
    }

    #[tokio::test]
    async fn per_node_on_error_overrides_policy() {
        // DAG policy fails fast, but the failing node is told to skip its dependents,
        // so the independent branch still completes.
        let dag = Dag::new()
            .add_node(FailNode { id: "a".into() })
            .add_node(AddNode {
                id: "b".into(),
                value: 1,
            })
            .add_node(AddNode {
                id: "c".into(),
                value: 2,
            })
            .add_edge("a", "b")
            .unwrap()
            .with_failure_policy(FailurePolicy::FailFast)
            .with_on_error("a", OnError::Skip);

        let result = dag.run(CancellationToken::new()).await.unwrap();

        assert_eq!(result.status("a"), Some(NodeStatus::Failed));
        assert_eq!(result.status("b"), Some(NodeStatus::Skipped));
        assert_eq!(result.status("c"), Some(NodeStatus::Completed));
    }

    #[tokio::test]
    async fn run_all_success_is_success() {
        let dag = Dag::new()
            .add_node(AddNode {
                id: "a".into(),
                value: 1,
            })
            .add_node(AddNode {
                id: "b".into(),
                value: 1,
            })
            .add_edge("a", "b")
            .unwrap();

        let result = dag.run(CancellationToken::new()).await.unwrap();

        assert!(result.is_success());
        assert_eq!(result.status("a"), Some(NodeStatus::Completed));
        assert_eq!(result.status("b"), Some(NodeStatus::Completed));
    }

    #[tokio::test]
    async fn execute_surfaces_the_fail_fast_error_not_a_non_aborting_one() {
        // A `Continue` node fails first but must not mask the error from the fail-fast node that
        // actually aborts the run.
        let dag = Dag::new()
            .with_failure_policy(FailurePolicy::FailFast)
            .add_node(MessageFailNode {
                id: "continue_node".into(),
                message: "continue-failure".into(),
            })
            .add_node(MessageFailNode {
                id: "fail_node".into(),
                message: "fail-fast-failure".into(),
            })
            .with_on_error("continue_node", OnError::Continue);

        let error = dag.execute(CancellationToken::new()).await.unwrap_err();
        assert!(
            error.to_string().contains("fail-fast-failure"),
            "expected the aborting error, got: {error}"
        );
        assert!(!error.to_string().contains("continue-failure"));
    }

    #[tokio::test]
    async fn fail_fast_aborts_independent_node_that_ignores_cancellation() {
        let dag = Dag::new()
            .add_node(FailNode { id: "boom".into() })
            .add_node(SleepNode {
                id: "slow".into(),
                duration: Duration::from_secs(30),
            });

        // The slow, independent node ignores the token; fail-fast must abort it rather than wait.
        let result =
            tokio::time::timeout(Duration::from_secs(5), dag.run(CancellationToken::new()))
                .await
                .expect("fail-fast must abort the token-ignoring node, not block on it")
                .unwrap();

        assert_eq!(result.status("boom"), Some(NodeStatus::Failed));
        assert_eq!(result.status("slow"), Some(NodeStatus::Canceled));
        assert!(!result.is_success());
    }

    #[tokio::test]
    async fn run_duration_is_wall_clock_not_sum_of_concurrent_nodes() {
        let dag = Dag::new()
            .with_max_parallelism(2)
            .add_node(SleepNode {
                id: "a".into(),
                duration: Duration::from_millis(100),
            })
            .add_node(SleepNode {
                id: "b".into(),
                duration: Duration::from_millis(100),
            });

        let result = dag.run(CancellationToken::new()).await.unwrap();

        // Two concurrent 100ms nodes complete in ~100ms wall-clock, well below the ~200ms sum.
        assert!(
            result.duration_ms < 180,
            "duration_ms={} looks like a per-node sum",
            result.duration_ms
        );
        assert!(result.nodes["a"].duration_ms >= 50);
        assert!(result.nodes["b"].duration_ms >= 50);
    }

    #[tokio::test]
    async fn unknown_on_error_override_is_rejected() {
        let dag = Dag::new()
            .add_node(AddNode {
                id: "a".into(),
                value: 1,
            })
            .with_on_error("does_not_exist", OnError::Skip);

        let error = dag.run(CancellationToken::new()).await.unwrap_err();
        assert_eq!(error.code(), ErrorCode::InvalidInput);
        let error = dag.execute(CancellationToken::new()).await.unwrap_err();
        assert_eq!(error.code(), ErrorCode::InvalidInput);
    }
}
