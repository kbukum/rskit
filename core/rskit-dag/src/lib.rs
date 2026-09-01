//! DAG task orchestrator with parallel execution and `as_tool` adapter.
//!
//! Provides a directed acyclic graph runner that schedules independent nodes in parallel
//! and passes outputs downstream. Includes an `as_tool` adapter
//! so DAG workflows can be exposed as LLM tool-call targets.
#![warn(missing_docs)]

mod as_tool;
mod dag;
mod node;
mod result;

pub use as_tool::{DagTool, ToolConfig, as_tool};
pub use dag::{Dag, FailurePolicy};
pub use node::{DagNode, TypedDagNode};
pub use result::{DagResult, NodeResult, NodeStatus, OnError};
