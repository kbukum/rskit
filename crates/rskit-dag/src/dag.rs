use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::Arc;

use crate::node::DagNode;
use rskit_errors::{AppError, AppResult, ErrorCode};
use tokio::sync::Mutex;
use tokio::task::JoinSet;
use tokio_util::sync::CancellationToken;

/// A directed acyclic graph of executable nodes.
///
/// Nodes are executed in topological order with maximum parallelism:
/// all nodes whose dependencies are satisfied run concurrently.
pub struct Dag {
    nodes: HashMap<String, Arc<dyn DagNode>>,
    /// Forward edges: `from_id` → downstream dependents
    edges: HashMap<String, Vec<String>>,
    /// Reverse edges: `to_id` → upstream dependencies
    reverse_edges: HashMap<String, Vec<String>>,
}

impl Default for Dag {
    fn default() -> Self {
        Self::new()
    }
}

impl Dag {
    /// Create an empty DAG.
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            edges: HashMap::new(),
            reverse_edges: HashMap::new(),
        }
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
    ///
    /// This means `from` must complete before `to` can execute,
    /// and `to` will receive `from`'s output in its inputs.
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
    ///
    /// Returns an error if the graph contains a cycle.
    pub fn topological_sort(&self) -> AppResult<Vec<String>> {
        let mut in_degree: HashMap<String, usize> = HashMap::new();
        for id in self.nodes.keys() {
            in_degree.entry(id.clone()).or_insert(0);
        }
        for deps in self.reverse_edges.values() {
            // in_degree is already initialized for all nodes
            for _ in deps {
                // counted via edges below
            }
        }
        // Compute in-degrees from reverse_edges
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
    /// Nodes with no pending dependencies are launched concurrently.
    /// When a node completes, any downstream nodes whose dependencies are
    /// now fully satisfied are started immediately.
    pub async fn execute(
        &self,
        cancel: CancellationToken,
    ) -> AppResult<HashMap<String, serde_json::Value>> {
        // Validate the DAG is acyclic first
        let _ = self.topological_sort()?;

        let outputs: Arc<Mutex<HashMap<String, serde_json::Value>>> =
            Arc::new(Mutex::new(HashMap::new()));

        // Track remaining in-degree for scheduling
        let mut remaining_in_degree: HashMap<String, usize> = HashMap::new();
        for (id, deps) in &self.reverse_edges {
            remaining_in_degree.insert(id.clone(), deps.len());
        }
        // Ensure all nodes are in the map
        for id in self.nodes.keys() {
            remaining_in_degree.entry(id.clone()).or_insert(0);
        }

        let mut join_set: JoinSet<AppResult<(String, serde_json::Value)>> = JoinSet::new();
        let mut pending: HashSet<String> = HashSet::new();
        let mut completed: HashSet<String> = HashSet::new();

        // Launch all root nodes (in-degree == 0)
        for (id, deg) in &remaining_in_degree {
            if *deg == 0 {
                let node = Arc::clone(self.nodes.get(id).unwrap());
                let cancel = cancel.clone();
                let outputs = Arc::clone(&outputs);
                let reverse = self.reverse_edges.get(id).cloned().unwrap_or_default();
                let node_id = id.clone();

                pending.insert(id.clone());
                join_set.spawn(async move {
                    let inputs = {
                        let out = outputs.lock().await;
                        reverse
                            .iter()
                            .filter_map(|dep_id| {
                                out.get(dep_id).map(|v| (dep_id.clone(), v.clone()))
                            })
                            .collect()
                    };

                    tracing::debug!(node = %node_id, "executing DAG node");
                    let result = node.execute(inputs, cancel).await?;
                    Ok((node_id, result))
                });
            }
        }

        // Process completions and schedule downstream nodes
        while let Some(result) = join_set.join_next().await {
            let (finished_id, value) = result.map_err(|e| {
                AppError::new(ErrorCode::Internal, format!("DAG task panicked: {e}"))
            })??;

            tracing::debug!(node = %finished_id, "DAG node completed");
            {
                let mut out = outputs.lock().await;
                out.insert(finished_id.clone(), value);
            }
            pending.remove(&finished_id);
            completed.insert(finished_id.clone());

            // Check downstream nodes
            if let Some(dependents) = self.edges.get(&finished_id) {
                for dep_id in dependents {
                    if completed.contains(dep_id) || pending.contains(dep_id) {
                        continue;
                    }
                    if let Some(deg) = remaining_in_degree.get_mut(dep_id) {
                        *deg -= 1;
                        if *deg == 0 {
                            let node = Arc::clone(self.nodes.get(dep_id).unwrap());
                            let cancel = cancel.clone();
                            let outputs = Arc::clone(&outputs);
                            let reverse =
                                self.reverse_edges.get(dep_id).cloned().unwrap_or_default();
                            let node_id = dep_id.clone();

                            pending.insert(dep_id.clone());
                            join_set.spawn(async move {
                                let inputs = {
                                    let out = outputs.lock().await;
                                    reverse
                                        .iter()
                                        .filter_map(|dep| {
                                            out.get(dep).map(|v| (dep.clone(), v.clone()))
                                        })
                                        .collect()
                                };

                                tracing::debug!(node = %node_id, "executing DAG node");
                                let result = node.execute(inputs, cancel).await?;
                                Ok((node_id, result))
                            });
                        }
                    }
                }
            }
        }

        let final_outputs = Arc::try_unwrap(outputs)
            .map_err(|_| AppError::new(ErrorCode::Internal, "failed to unwrap DAG outputs"))?
            .into_inner();

        Ok(final_outputs)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::future::Future;
    use std::pin::Pin;

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

        let cancel = CancellationToken::new();
        let outputs = dag.execute(cancel).await.unwrap();

        assert_eq!(outputs["a"], serde_json::json!(1));
        assert_eq!(outputs["b"], serde_json::json!(3)); // 1 + 2
        assert_eq!(outputs["c"], serde_json::json!(6)); // 3 + 3
    }

    #[tokio::test]
    async fn diamond_dag_merges_inputs() {
        //   a
        //  / \
        // b   c
        //  \ /
        //   d
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

        let cancel = CancellationToken::new();
        let outputs = dag.execute(cancel).await.unwrap();

        assert_eq!(outputs["a"], serde_json::json!(10));
        assert_eq!(outputs["b"], serde_json::json!(11)); // 10 + 1
        assert_eq!(outputs["c"], serde_json::json!(12)); // 10 + 2
        assert_eq!(outputs["d"], serde_json::json!(23)); // 11 + 12 + 0
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

        let dag = dag.add_edge("a", "b").unwrap().add_edge("b", "a").unwrap();

        let result = dag.topological_sort();
        assert!(result.is_err());
    }

    #[test]
    fn topological_sort_linear() {
        let dag = Dag::new()
            .add_node(AddNode {
                id: "a".into(),
                value: 0,
            })
            .add_node(AddNode {
                id: "b".into(),
                value: 0,
            })
            .add_node(AddNode {
                id: "c".into(),
                value: 0,
            });

        let dag = dag.add_edge("a", "b").unwrap().add_edge("b", "c").unwrap();

        let sorted = dag.topological_sort().unwrap();
        let pos_a = sorted.iter().position(|x| x == "a").unwrap();
        let pos_b = sorted.iter().position(|x| x == "b").unwrap();
        let pos_c = sorted.iter().position(|x| x == "c").unwrap();
        assert!(pos_a < pos_b);
        assert!(pos_b < pos_c);
    }
}
