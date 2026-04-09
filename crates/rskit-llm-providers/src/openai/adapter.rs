//! Adapter factory: bridges OpenAI [`Config`] → [`LlmProvider`] via rskit-httpclient.

use async_trait::async_trait;
use rskit_errors::{AppError, AppResult, ErrorCode};
use rskit_httpclient::{Auth, HttpClient, HttpClientConfig, Request};
use rskit_llm::LlmProvider;
use rskit_llm::types::{CompletionRequest, CompletionResponse};

use super::config::Config;
use super::dialect::OpenAiDialect;

/// An [`LlmProvider`] backed by the OpenAI chat-completions API.
pub struct OpenAiAdapter {
    client: HttpClient,
    model: String,
}

/// Create a new [`LlmProvider`] wired to OpenAI with Bearer auth.
pub fn new_adapter(cfg: &Config) -> AppResult<OpenAiAdapter> {
    let http_cfg = HttpClientConfig::new()
        .with_base_url(&cfg.base_url)
        .with_auth(Auth::bearer(&cfg.api_key));

    let client = HttpClient::new(http_cfg)?;

    Ok(OpenAiAdapter {
        client,
        model: cfg.model.clone(),
    })
}

#[async_trait]
impl LlmProvider for OpenAiAdapter {
    async fn complete(&self, mut req: CompletionRequest) -> AppResult<CompletionResponse> {
        if req.model.is_empty() {
            req.model.clone_from(&self.model);
        }

        let body = OpenAiDialect::build_body(&req)?;

        let request = Request::post(OpenAiDialect::endpoint())
            .json_body(&body)
            .map_err(|e| {
                AppError::new(ErrorCode::Internal, format!("failed to build request: {e}"))
            })?;

        let response = self.client.send(request).await?;

        if !response.is_success() {
            let status = response.status().as_u16();
            let text = response.text().unwrap_or_default();
            return Err(OpenAiDialect::parse_error(status, &text));
        }

        let text = response.text().map_err(|e| {
            AppError::new(
                ErrorCode::ExternalService,
                format!("failed to read OpenAI response: {e}"),
            )
        })?;

        OpenAiDialect::parse_response(&text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_adapter_constructs_successfully() {
        let cfg = Config {
            api_key: "sk-test".into(),
            base_url: "https://api.openai.com/v1".into(),
            model: "gpt-4o".into(),
            embedding_model: "text-embedding-3-small".into(),
            embedding_dimensions: 1536,
        };
        let adapter = new_adapter(&cfg);
        assert!(adapter.is_ok());
    }

    #[test]
    fn adapter_is_object_safe() {
        let cfg = Config {
            api_key: "sk-test".into(),
            base_url: "https://api.openai.com/v1".into(),
            model: "gpt-4o".into(),
            embedding_model: "text-embedding-3-small".into(),
            embedding_dimensions: 1536,
        };
        let adapter = new_adapter(&cfg).unwrap();
        let _boxed: Box<dyn LlmProvider> = Box::new(adapter);
    }
}
