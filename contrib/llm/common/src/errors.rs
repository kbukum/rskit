//! Common error parsing and token estimation utilities.

use rskit_ai::GenAiError;
use rskit_errors::{AppError, ErrorCode};
use serde::Deserialize;

/// Structured API error returned by LLM providers.
#[derive(Debug, Clone)]
pub struct ApiError {
    /// HTTP status code from the provider.
    pub status: u16,
    /// Provider name (e.g. "openai", "anthropic", "gemini").
    pub provider: String,
    /// Error message extracted from the response body.
    pub message: String,
    /// Optional error type/code from the provider.
    pub error_type: Option<String>,
}

impl std::fmt::Display for ApiError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{} API error (HTTP {}): {}",
            self.provider, self.status, self.message
        )
    }
}

impl std::error::Error for ApiError {}

impl ApiError {
    /// Map provider wire errors to canonical GenAI sentinels at the adapter boundary.
    #[must_use]
    pub fn to_genai_error(&self) -> GenAiError {
        let kind = self
            .error_type
            .as_deref()
            .unwrap_or_default()
            .to_ascii_lowercase();
        if self.status == 429 || kind.contains("rate") {
            GenAiError::RateLimited
        } else if self.status == 404 || kind.contains("model_not_found") {
            GenAiError::ModelNotFound
        } else if kind.contains("context") || kind.contains("token") && kind.contains("limit") {
            GenAiError::ContextLengthExceeded
        } else if kind.contains("content") || kind.contains("safety") || kind.contains("filter") {
            GenAiError::ContentFilter
        } else if self.status == 503 || kind.contains("overloaded") {
            GenAiError::ModelOverloaded
        } else {
            GenAiError::InvalidRequest(self.message.clone())
        }
    }
}

impl From<ApiError> for AppError {
    fn from(e: ApiError) -> Self {
        let code = match e.status {
            401 => ErrorCode::Unauthorized,
            403 => ErrorCode::Forbidden,
            404 => ErrorCode::NotFound,
            429 => ErrorCode::RateLimited,
            _ => ErrorCode::ExternalService,
        };
        AppError::new(code, e.to_string())
            .with_detail("provider", e.provider)
            .with_detail("status", e.status.to_string())
    }
}

// --- OpenAI error body ---

#[derive(Deserialize)]
struct OpenAiErrorBody {
    error: Option<OpenAiErrorDetail>,
}

#[derive(Deserialize)]
struct OpenAiErrorDetail {
    message: Option<String>,
    #[serde(rename = "type")]
    error_type: Option<String>,
}

/// Parse an OpenAI-style error response body into an [`ApiError`].
pub fn parse_openai_error(status: u16, body: &str) -> ApiError {
    let (message, error_type) = serde_json::from_str::<OpenAiErrorBody>(body)
        .ok()
        .and_then(|b| b.error)
        .map(|e| (e.message.unwrap_or_else(|| body.to_string()), e.error_type))
        .unwrap_or_else(|| (body.to_string(), None));

    ApiError {
        status,
        provider: "openai".to_string(),
        message,
        error_type,
    }
}

// --- Anthropic error body ---

#[derive(Deserialize)]
struct AnthropicErrorBody {
    error: Option<AnthropicErrorDetail>,
}

#[derive(Deserialize)]
struct AnthropicErrorDetail {
    message: Option<String>,
    #[serde(rename = "type")]
    error_type: Option<String>,
}

/// Parse an Anthropic-style error response body into an [`ApiError`].
pub fn parse_anthropic_error(status: u16, body: &str) -> ApiError {
    let (message, error_type) = serde_json::from_str::<AnthropicErrorBody>(body)
        .ok()
        .and_then(|b| b.error)
        .map(|e| (e.message.unwrap_or_else(|| body.to_string()), e.error_type))
        .unwrap_or_else(|| (body.to_string(), None));

    ApiError {
        status,
        provider: "anthropic".to_string(),
        message,
        error_type,
    }
}

// --- Gemini error body ---

#[derive(Deserialize)]
struct GeminiErrorBody {
    error: Option<GeminiErrorDetail>,
}

#[derive(Deserialize)]
struct GeminiErrorDetail {
    message: Option<String>,
    status: Option<String>,
}

/// Parse a Google Gemini-style error response body into an [`ApiError`].
pub fn parse_gemini_error(status: u16, body: &str) -> ApiError {
    let (message, error_type) = serde_json::from_str::<GeminiErrorBody>(body)
        .ok()
        .and_then(|b| b.error)
        .map(|e| (e.message.unwrap_or_else(|| body.to_string()), e.status))
        .unwrap_or_else(|| (body.to_string(), None));

    ApiError {
        status,
        provider: "gemini".to_string(),
        message,
        error_type,
    }
}

/// Rough token estimator (~4 chars per token).
///
/// Use when a dedicated tokenizer is unavailable.
pub fn estimate_tokens(text: &str) -> usize {
    text.len() / 4
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_openai_error_structured() {
        let body = r#"{"error":{"message":"Rate limit exceeded","type":"rate_limit_error"}}"#;
        let err = parse_openai_error(429, body);
        assert_eq!(err.status, 429);
        assert_eq!(err.provider, "openai");
        assert_eq!(err.message, "Rate limit exceeded");
        assert_eq!(err.error_type.as_deref(), Some("rate_limit_error"));
    }

    #[test]
    fn parse_openai_error_plain_text() {
        let err = parse_openai_error(500, "internal error");
        assert_eq!(err.message, "internal error");
        assert!(err.error_type.is_none());
    }

    #[test]
    fn parse_anthropic_error_structured() {
        let body = r#"{"error":{"message":"Invalid API key","type":"authentication_error"}}"#;
        let err = parse_anthropic_error(401, body);
        assert_eq!(err.status, 401);
        assert_eq!(err.provider, "anthropic");
        assert_eq!(err.message, "Invalid API key");
    }

    #[test]
    fn parse_anthropic_error_plain_text() {
        let err = parse_anthropic_error(500, "server error");
        assert_eq!(err.message, "server error");
    }

    #[test]
    fn parse_gemini_error_structured() {
        let body =
            r#"{"error":{"message":"API key not valid","code":400,"status":"INVALID_ARGUMENT"}}"#;
        let err = parse_gemini_error(400, body);
        assert_eq!(err.status, 400);
        assert_eq!(err.provider, "gemini");
        assert_eq!(err.message, "API key not valid");
        assert_eq!(err.error_type.as_deref(), Some("INVALID_ARGUMENT"));
    }

    #[test]
    fn parse_gemini_error_plain_text() {
        let err = parse_gemini_error(500, "oops");
        assert_eq!(err.message, "oops");
    }

    #[test]
    fn api_error_into_app_error() {
        let api_err = ApiError {
            status: 429,
            provider: "openai".to_string(),
            message: "rate limited".to_string(),
            error_type: None,
        };
        assert_eq!(api_err.to_genai_error(), GenAiError::RateLimited);
        let app_err: AppError = api_err.into();
        assert_eq!(app_err.code, ErrorCode::RateLimited);
    }

    #[test]
    fn estimate_tokens_basic() {
        assert_eq!(estimate_tokens("hello world!"), 3); // 12 chars / 4
        assert_eq!(estimate_tokens(""), 0);
    }
}
