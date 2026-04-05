//! Provider adapters connecting messaging to the provider pattern.
//!
//! Once messaging components are expressed as providers (Sink, StreamProvider),
//! they compose naturally with all other kit patterns that accept providers:
//! DAG (`DagNode::from_provider`), Worker (`from_provider`), Pipeline, etc.

#[cfg(feature = "provider-bridge")]
pub mod provider;
