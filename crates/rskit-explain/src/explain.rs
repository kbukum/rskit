//! Explanation generation using an LLM provider.

use rskit_errors::{AppError, AppResult, ErrorCode};
use rskit_llm::LlmProvider;
use rskit_llm::types::{CompletionRequest, system, user};

use crate::types::{Explanation, Request};

const DEFAULT_TEMPLATE: &str = r#"You are an expert analyst. Given the following signals, produce a structured JSON explanation.

Signals:
{signals}
{context}
Respond with ONLY valid JSON (no markdown fences) in this exact schema:
{{
  "summary": "string — high-level summary",
  "reasoning": [
    {{
      "signal": "signal name",
      "finding": "what you found",
      "impact": "how it affects the outcome"
    }}
  ],
  "key_factors": ["factor1", "factor2"],
  "confidence": 0.0
}}"#;

/// Generate a structured explanation from signals using an LLM provider.
pub async fn generate(provider: &dyn LlmProvider, request: Request) -> AppResult<Explanation> {
    let signals_text = request
        .signals
        .iter()
        .map(|s| format!("- {} ({}): {}", s.label, s.name, s.value))
        .collect::<Vec<_>>()
        .join("\n");

    let context_text = request
        .context
        .as_deref()
        .map(|c| format!("\nAdditional context: {c}\n"))
        .unwrap_or_default();

    let prompt = request
        .template
        .as_deref()
        .unwrap_or(DEFAULT_TEMPLATE)
        .replace("{signals}", &signals_text)
        .replace("{context}", &context_text);

    let req = CompletionRequest {
        model: String::new(),
        messages: vec![
            system("You are a precise analytical assistant. Always respond with valid JSON."),
            user(&prompt),
        ],
        max_tokens: request.max_tokens.or(Some(1024)),
        temperature: Some(0.2),
        stream: false,
        tools: None,
        tool_choice: None,
    };

    let response = provider.complete(req).await?;
    let text = response.text();

    parse_explanation(&text)
}

/// Parse an explanation from LLM response text, handling optional code fences.
fn parse_explanation(text: &str) -> AppResult<Explanation> {
    let trimmed = text.trim();

    // Strip markdown code fences if present
    let json_str = if trimmed.starts_with("```") {
        let without_opening = trimmed
            .strip_prefix("```json")
            .or_else(|| trimmed.strip_prefix("```"))
            .unwrap_or(trimmed);
        without_opening
            .strip_suffix("```")
            .unwrap_or(without_opening)
            .trim()
    } else {
        trimmed
    };

    serde_json::from_str(json_str).map_err(|e| {
        AppError::new(
            ErrorCode::InvalidFormat,
            format!("failed to parse explanation JSON: {e}"),
        )
    })
}

#[cfg(test)]
mod tests {
    use async_trait::async_trait;
    use rskit_llm::types::{
        AssistantMessage, CompletionRequest, CompletionResponse, ContentBlock, StopReason, Usage,
    };

    use super::*;

    struct MockProvider {
        response_text: String,
    }

    #[async_trait]
    impl LlmProvider for MockProvider {
        async fn complete(&self, _req: CompletionRequest) -> AppResult<CompletionResponse> {
            Ok(CompletionResponse {
                message: AssistantMessage {
                    content: vec![ContentBlock::Text {
                        text: self.response_text.clone(),
                    }],
                    tool_calls: vec![],
                    usage: None,
                },
                model: "mock".to_string(),
                usage: Usage {
                    input_tokens: 10,
                    output_tokens: 20,
                },
                stop_reason: Some(StopReason::EndTurn),
            })
        }
    }

    fn sample_json() -> String {
        serde_json::json!({
            "summary": "High engagement detected",
            "reasoning": [
                {
                    "signal": "engagement_rate",
                    "finding": "Rate is above average",
                    "impact": "Positive indicator for content quality"
                }
            ],
            "key_factors": ["engagement_rate"],
            "confidence": 0.85
        })
        .to_string()
    }

    #[tokio::test]
    async fn generate_parses_clean_json() {
        let provider = MockProvider {
            response_text: sample_json(),
        };

        let request = Request {
            signals: vec![crate::types::Signal {
                name: "engagement_rate".into(),
                value: 0.75,
                label: "Engagement Rate".into(),
            }],
            template: None,
            max_tokens: None,
            context: None,
        };

        let result = generate(&provider, request).await.unwrap();
        assert_eq!(result.summary, "High engagement detected");
        assert_eq!(result.reasoning.len(), 1);
        assert_eq!(result.key_factors, vec!["engagement_rate"]);
        assert!((result.confidence - 0.85).abs() < 1e-6);
    }

    #[tokio::test]
    async fn generate_handles_code_fence_wrapping() {
        let provider = MockProvider {
            response_text: format!("```json\n{}\n```", sample_json()),
        };

        let request = Request {
            signals: vec![crate::types::Signal {
                name: "score".into(),
                value: 0.5,
                label: "Score".into(),
            }],
            template: None,
            max_tokens: None,
            context: None,
        };

        let result = generate(&provider, request).await.unwrap();
        assert_eq!(result.summary, "High engagement detected");
    }

    #[tokio::test]
    async fn generate_handles_bare_code_fence() {
        let provider = MockProvider {
            response_text: format!("```\n{}\n```", sample_json()),
        };

        let request = Request {
            signals: vec![],
            template: None,
            max_tokens: None,
            context: None,
        };

        let result = generate(&provider, request).await.unwrap();
        assert_eq!(result.summary, "High engagement detected");
    }

    #[tokio::test]
    async fn generate_with_custom_template() {
        let provider = MockProvider {
            response_text: sample_json(),
        };

        let request = Request {
            signals: vec![crate::types::Signal {
                name: "x".into(),
                value: 1.0,
                label: "X".into(),
            }],
            template: Some("Custom: {signals}{context}".into()),
            max_tokens: Some(512),
            context: Some("extra context".into()),
        };

        let result = generate(&provider, request).await.unwrap();
        assert_eq!(result.summary, "High engagement detected");
    }

    #[test]
    fn parse_explanation_invalid_json() {
        let result = parse_explanation("not json at all");
        assert!(result.is_err());
    }

    #[test]
    fn parse_explanation_valid_json() {
        let result = parse_explanation(&sample_json());
        assert!(result.is_ok());
    }
}
