//! Serializable DAG execution results and node-level error policy.

use std::collections::{BTreeMap, HashMap};

use serde::{Deserialize, Serialize};

/// Terminal status of a single node after a DAG run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum NodeStatus {
    /// The node executed successfully.
    Completed,
    /// The node returned an error.
    Failed,
    /// The node was skipped because an upstream dependency failed.
    Skipped,
    /// The node did not run because the DAG run was aborted or cancelled.
    Canceled,
    /// The node exceeded its execution deadline.
    TimedOut,
}

/// Per-node override for how a node failure affects the rest of the DAG run.
///
/// A node without an explicit override inherits the DAG-level
/// [`FailurePolicy`](crate::FailurePolicy).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum OnError {
    /// Abort the whole run when this node fails.
    Fail,
    /// Skip this node's downstream dependents, but keep independent branches running.
    Skip,
    /// Run downstream dependents anyway, with the failed node's output absent.
    Continue,
}

/// Result of executing a single DAG node.
// `Eq` cannot be derived: `output` holds a `serde_json::Value`, which is not `Eq`.
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NodeResult {
    /// Node identifier.
    pub name: String,
    /// Terminal status of the node.
    pub status: NodeStatus,
    /// Wall-clock execution time in milliseconds (`0` for nodes that never ran).
    pub duration_ms: u64,
    /// Serialized node output, present for [`NodeStatus::Completed`] nodes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub output: Option<serde_json::Value>,
    /// Error message, present for failed, timed-out, or cancelled nodes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

/// Result of executing a whole DAG.
// `Eq` cannot be derived: node outputs hold `serde_json::Value`, which is not `Eq`.
#[allow(clippy::derive_partial_eq_without_eq)]
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DagResult {
    /// Per-node results keyed by node id, ordered deterministically by id.
    pub nodes: BTreeMap<String, NodeResult>,
    /// Total wall-clock run time in milliseconds.
    pub duration_ms: u64,
}

impl DagResult {
    /// Return the terminal status of a node, if it is part of this result.
    #[must_use]
    pub fn status(&self, node_id: &str) -> Option<NodeStatus> {
        self.nodes.get(node_id).map(|result| result.status)
    }

    /// Return the outputs of every successfully completed node keyed by node id.
    #[must_use]
    pub fn outputs(&self) -> HashMap<String, serde_json::Value> {
        self.nodes
            .iter()
            .filter(|(_, result)| result.status == NodeStatus::Completed)
            .filter_map(|(id, result)| result.output.clone().map(|output| (id.clone(), output)))
            .collect()
    }

    /// Return `true` when every node completed successfully.
    #[must_use]
    pub fn is_success(&self) -> bool {
        self.nodes
            .values()
            .all(|result| result.status == NodeStatus::Completed)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> DagResult {
        let mut nodes = BTreeMap::new();
        nodes.insert(
            "load".to_string(),
            NodeResult {
                name: "load".to_string(),
                status: NodeStatus::Completed,
                duration_ms: 12,
                output: Some(serde_json::json!({"rows": 3})),
                error: None,
            },
        );
        nodes.insert(
            "transform".to_string(),
            NodeResult {
                name: "transform".to_string(),
                status: NodeStatus::Failed,
                duration_ms: 4,
                output: None,
                error: Some("boom".to_string()),
            },
        );
        nodes.insert(
            "publish".to_string(),
            NodeResult {
                name: "publish".to_string(),
                status: NodeStatus::Skipped,
                duration_ms: 0,
                output: None,
                error: None,
            },
        );
        DagResult {
            nodes,
            duration_ms: 20,
        }
    }

    #[test]
    fn node_status_and_on_error_use_snake_case_wire_strings() {
        assert_eq!(
            serde_json::to_value(NodeStatus::TimedOut).unwrap(),
            serde_json::json!("timed_out")
        );
        assert_eq!(
            serde_json::to_value(NodeStatus::Canceled).unwrap(),
            serde_json::json!("canceled")
        );
        assert_eq!(
            serde_json::to_value(OnError::Continue).unwrap(),
            serde_json::json!("continue")
        );
    }

    #[test]
    fn dag_result_matches_cross_kit_golden_json() {
        let actual = serde_json::to_string_pretty(&sample()).unwrap();
        let expected = include_str!("../tests/fixtures/cross-kit/dag/dag-result.json");
        assert_eq!(format!("{actual}\n"), expected);

        let decoded: DagResult = serde_json::from_str(expected).unwrap();
        assert_eq!(decoded, sample());
    }

    #[test]
    fn helpers_expose_status_outputs_and_success() {
        let result = sample();
        assert_eq!(result.status("transform"), Some(NodeStatus::Failed));
        assert_eq!(result.status("missing"), None);
        assert!(!result.is_success());
        let outputs = result.outputs();
        assert_eq!(outputs.len(), 1);
        assert_eq!(outputs["load"], serde_json::json!({"rows": 3}));
    }
}
