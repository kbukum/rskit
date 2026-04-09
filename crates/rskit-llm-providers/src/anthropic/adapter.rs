//! Adapter factory: bridges Anthropic [`Config`] → [`LlmProvider`] via rskit-httpclient.

use async_trait::async_trait;
use rskit_errors::{AppError, AppResult, ErrorCode};
use rskit_httpclient::{Auth, HttpClient, HttpClientConfig, Request};
use rskit_llm::LlmProvider;
use rskit_llm::types::{CompletionRequest, CompletionResponse};

use super::config::Config;
use super::dialect::AnthropicDialect;

/// An [`LlmProvider`] backed by the Anthropic Messages API.
pub struct AnthropicAdapter {
    client: HttpClient,
    model: String,
    api_version: String,
}

/// Create a new [`LlmProvider`] wired to Anthropic with `x-api-key` + `anthropic-version` headers.
pub fn new_adapter(cfg: &Config) -> AppResult<AnthropicAdapter> {
    let http_cfg = HttpClientConfig::new()
        .with_base_url(&cfg.base_url)
        .with_auth(Auth::api_key("x-api-key", &cfg.api_key))
        .with_header("anthropic-version", &cfg.api_version);

    let client = HttpClient::new(http_cfg)?;

    Ok(AnthropicAdapter {
        client,
        model: cfg.model.clone(),
        api_version: cfg.api_version.clone(),
    })
}

#[async_trait]
impl LlmProvider for AnthropicAdapter {
    async fn complete(&self, mut req: CompletionRequest) -> AppResult<CompletionResponse> {
        if req.model.is_empty() {
            req.model.clone_from(&self.model);
        }

        let body = AnthropicDialect::build_body(&req)?;

        let request = Request::post(AnthropicDialect::endpoint())
            .header("anthropic-version", &self.api_version)
            .header("content-type", "application/json")
            .json_body(&body)
            .map_err(|e| {
                AppError::new(ErrorCode::Internal, format!("failed to build request: {e}"))
            })?;

        let response = self.client.send(request).await?;

        if !response.is_success() {
            let status = response.status().as_u16();
            let text = response.text().unwrap_or_default();
            return Err(AnthropicDialect::parse_error(status, &text));
        }

        let text = response.text().map_err(|e| {
            AppError::new(
                ErrorCode::ExternalService,
                format!("failed to read Anthropic response: {e}"),
            )
        })?;

        AnthropicDialect::parse_response(&text)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_adapter_constructs_successfully() {
        let cfg = Config {
            api_key: "sk-ant-test".into(),
            base_url: "https://api.anthropic.com".into(),
            model: "claude-sonnet-4-20250514".into(),
            api_version: "2023-06-01".into(),
        };
        let adapter = new_adapter(&cfg);
        assert!(adapter.is_ok());
    }

    #[test]
    fn adapter_is_object_safe() {
        let cfg = Config {
            api_key: "sk-ant-test".into(),
            base_url: "https://api.anthropic.com".into(),
            model: "claude-sonnet-4-20250514".into(),
            api_version: "2023-06-01".into(),
        };
        let adapter = new_adapter(&cfg).unwrap();
        let _boxed: Box<dyn LlmProvider> = Box::new(adapter);
    }
}
