//! Types for structured explanation generation.

use serde::{Deserialize, Serialize};

/// A named numeric signal with a human-readable label.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Signal {
    /// Signal identifier (e.g., `"engagement_rate"`).
    pub name: String,
    /// Numeric value of the signal.
    pub value: f64,
    /// Human-readable label (e.g., `"Engagement Rate"`).
    pub label: String,
}

/// Request for generating a structured explanation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Request {
    /// Input signals to explain.
    pub signals: Vec<Signal>,
    /// Optional custom prompt template. Use `{signals}` as placeholder.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub template: Option<String>,
    /// Maximum tokens for the LLM response.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_tokens: Option<u32>,
    /// Additional context to include in the prompt.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context: Option<String>,
}

/// A structured explanation produced by the LLM.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Explanation {
    /// High-level summary of the explanation.
    pub summary: String,
    /// Step-by-step reasoning about each signal.
    pub reasoning: Vec<ReasoningStep>,
    /// Key factors that drive the outcome.
    pub key_factors: Vec<String>,
    /// Confidence score in `[0.0, 1.0]`.
    pub confidence: f64,
}

/// A single reasoning step about a specific signal.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReasoningStep {
    /// Which signal this step refers to.
    pub signal: String,
    /// What was found about this signal.
    pub finding: String,
    /// How this signal impacts the overall outcome.
    pub impact: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn signal_serde_roundtrip() {
        let signal = Signal {
            name: "score".into(),
            value: 0.85,
            label: "Score".into(),
        };
        let json = serde_json::to_string(&signal).unwrap();
        let back: Signal = serde_json::from_str(&json).unwrap();
        assert_eq!(back.name, "score");
        assert!((back.value - 0.85).abs() < 1e-6);
    }

    #[test]
    fn request_serde_roundtrip() {
        let req = Request {
            signals: vec![Signal {
                name: "a".into(),
                value: 1.0,
                label: "A".into(),
            }],
            template: None,
            max_tokens: Some(500),
            context: None,
        };
        let json = serde_json::to_string(&req).unwrap();
        let back: Request = serde_json::from_str(&json).unwrap();
        assert_eq!(back.signals.len(), 1);
        assert_eq!(back.max_tokens, Some(500));
    }

    #[test]
    fn explanation_serde_roundtrip() {
        let explanation = Explanation {
            summary: "Good performance".into(),
            reasoning: vec![ReasoningStep {
                signal: "score".into(),
                finding: "High score".into(),
                impact: "Positive".into(),
            }],
            key_factors: vec!["score".into()],
            confidence: 0.9,
        };
        let json = serde_json::to_string(&explanation).unwrap();
        let back: Explanation = serde_json::from_str(&json).unwrap();
        assert_eq!(back.summary, "Good performance");
        assert_eq!(back.reasoning.len(), 1);
        assert!((back.confidence - 0.9).abs() < 1e-6);
    }
}
