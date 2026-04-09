//! Adapter factory: bridges Gemini [`Config`] → [`LlmProvider`] via rskit-httpclient.
//!
//! Gemini authenticates via API key as a query parameter (`?key=API_KEY`).

use async_trait::async_trait;
use rskit_errors::{AppError, AppResult, ErrorCode};
use rskit_httpclient::{HttpClient, HttpClientConfig, Request};
use rskit_llm::LlmProvider;
use rskit_llm::types::{CompletionRequest, CompletionResponse};

use super::config::Config;
use super::dialect::GeminiDialect;

/// An [`LlmProvider`] backed by the Google Gemini API.
pub struct GeminiAdapter {
    client: HttpClient,
    model: String,
    api_key: String,
}

/// Create a new [`LlmProvider`] wired to Gemini with API key via query param.
pub fn new_adapter(cfg: &Config) -> AppResult<GeminiAdapter> {
    let http_cfg = HttpClientConfig::new().with_base_url(&cfg.base_url);

    let client = HttpClient::new(http_cfg)?;

    Ok(GeminiAdapter {
        client,
        model: cfg.model.clone(),
        api_key: cfg.api_key.clone(),
    })
}

#[async_trait]
impl LlmProvider for GeminiAdapter {
    async fn complete(&self, mut req: CompletionRequest) -> AppResult<CompletionResponse> {
        if req.model.is_empty() {
            req.model.clone_from(&self.model);
        }

        let model = &req.model;
        let body = GeminiDialect::build_body(&req)?;
        let endpoint = GeminiDialect::endpoint(model);

        let request = Request::post(endpoint)
            .query_param("key", &self.api_key)
            .json_body(&body)
            .map_err(|e| {
                AppError::new(ErrorCode::Internal, format!("failed to build request: {e}"))
            })?;

        let response = self.client.send(request).await?;

        if !response.is_success() {
            let status = response.status().as_u16();
            let text = response.text().unwrap_or_default();
            return Err(GeminiDialect::parse_error(status, &text));
        }

        let text = response.text().map_err(|e| {
            AppError::new(
                ErrorCode::ExternalService,
                format!("failed to read Gemini response: {e}"),
            )
        })?;

        GeminiDialect::parse_response(&text, model)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_adapter_constructs_successfully() {
        let cfg = Config {
            api_key: "AIza-test".into(),
            base_url: "https://generativelanguage.googleapis.com".into(),
            model: "gemini-2.5-flash".into(),
        };
        let adapter = new_adapter(&cfg);
        assert!(adapter.is_ok());
    }

    #[test]
    fn adapter_is_object_safe() {
        let cfg = Config {
            api_key: "AIza-test".into(),
            base_url: "https://generativelanguage.googleapis.com".into(),
            model: "gemini-2.5-flash".into(),
        };
        let adapter = new_adapter(&cfg).unwrap();
        let _boxed: Box<dyn LlmProvider> = Box::new(adapter);
    }
}
