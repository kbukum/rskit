//! Core data types for the bench framework.
//!
//! These types mirror gokit's `bench` core types, adapted with Rust idioms.
//! All types use serde for JSON serialization to ensure cross-language compatibility.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A labeled data point in an evaluation dataset.
///
/// Generic over label type `L` for type-safe label handling.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BenchSample<L = String> {
    /// Unique sample identifier.
    pub id: String,
    /// Raw input data (file contents).
    #[serde(skip)]
    pub input: Vec<u8>,
    /// Ground-truth label.
    pub label: L,
    /// Optional source reference.
    #[serde(default)]
    pub source: String,
    /// Additional metadata.
    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,
}

/// An evaluator's output for a single sample.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Prediction<L = String> {
    /// Reference back to the sample.
    #[serde(default)]
    pub sample_id: String,
    /// Predicted label.
    pub label: L,
    /// Primary confidence score (typically 0.0–1.0).
    pub score: f64,
    /// Per-label scores (for multi-class).
    #[serde(default)]
    pub scores: HashMap<String, f64>,
    /// Additional prediction metadata.
    #[serde(default)]
    pub metadata: HashMap<String, serde_json::Value>,
}

/// Pairs ground-truth sample with its prediction for metric computation.
#[derive(Debug, Clone)]
pub struct ScoredSample<L = String> {
    pub sample: BenchSample<L>,
    pub prediction: Prediction<L>,
}

/// Converts string labels from a manifest into typed labels.
pub type LabelMapper<L> = Box<dyn Fn(&str) -> AppResult<L> + Send + Sync>;

/// A string-passthrough label mapper for simple string-labeled datasets.
pub fn string_label_mapper() -> LabelMapper<String> {
    Box::new(|s| Ok(s.to_string()))
}

impl<L: Default> Default for Prediction<L> {
    fn default() -> Self {
        Self {
            sample_id: String::default(),
            label: L::default(),
            score: 0.0,
            scores: HashMap::new(),
            metadata: HashMap::new(),
        }
    }
}
