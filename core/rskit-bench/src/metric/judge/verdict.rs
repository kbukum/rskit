//! The structured verdict parsed from an untrusted judge reply.

use serde::Deserialize;

/// A judge's structured verdict for one prediction/reference pair.
///
/// Parsed from the model's reply as untrusted, structured output: `score` is required and must be a finite number in `[0, 1]`; `rationale` is optional. Unknown fields are ignored so a judge may add its own metadata without breaking parsing.
#[derive(Debug, Clone, Deserialize)]
pub struct JudgeVerdict {
    /// Score in `[0, 1]`; higher means a closer match to the reference.
    pub score: f64,
    /// Optional short justification supplied by the judge.
    #[serde(default)]
    pub rationale: Option<String>,
}
