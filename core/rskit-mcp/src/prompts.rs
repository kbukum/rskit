//! Static MCP prompt registration and dispatch.

use std::{future::Future, pin::Pin, sync::Arc};

use rmcp::model::{GetPromptRequestParams, GetPromptResult, Prompt};

type PromptFuture = Pin<Box<dyn Future<Output = Result<GetPromptResult, rmcp::ErrorData>> + Send>>;

/// Static MCP prompt registration.
pub struct PromptEntry {
    /// Prompt metadata exposed to clients.
    pub prompt: Prompt,
    pub(crate) handler: Arc<dyn Fn(GetPromptRequestParams) -> PromptFuture + Send + Sync>,
}

impl PromptEntry {
    /// Construct a prompt entry from prompt metadata and an async handler.
    pub fn new<F, Fut>(prompt: Prompt, handler: F) -> Self
    where
        F: Fn(GetPromptRequestParams) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<GetPromptResult, rmcp::ErrorData>> + Send + 'static,
    {
        Self {
            prompt,
            handler: Arc::new(move |request| Box::pin(handler(request))),
        }
    }
}

impl Clone for PromptEntry {
    fn clone(&self) -> Self {
        Self {
            prompt: self.prompt.clone(),
            handler: Arc::clone(&self.handler),
        }
    }
}

pub(crate) fn prompt_name(prompt: &Prompt) -> Option<String> {
    serde_json::to_value(prompt).ok().and_then(|value| {
        value
            .get("name")
            .and_then(|name| name.as_str())
            .map(str::to_string)
    })
}

pub(crate) fn invalid_params_error(message: String) -> rmcp::ErrorData {
    rmcp::ErrorData::new(rmcp::model::ErrorCode::INVALID_PARAMS, message, None)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[tokio::test]
    async fn prompt_entry_clones_metadata_and_invokes_handler() {
        let prompt: Prompt = serde_json::from_value(json!({
            "name": "summarize",
            "description": "Summarize input"
        }))
        .unwrap();
        let entry = PromptEntry::new(prompt, |request| async move {
            serde_json::from_value(json!({
                "description": "Summary",
                "messages": [{
                    "role": "user",
                    "content": {"type": "text", "text": request.name}
                }]
            }))
            .map_err(|err| invalid_params_error(err.to_string()))
        });
        let cloned = entry.clone();

        assert_eq!(prompt_name(&cloned.prompt).as_deref(), Some("summarize"));
        let request: GetPromptRequestParams = serde_json::from_value(json!({
            "name": "summarize"
        }))
        .unwrap();
        let result = (cloned.handler)(request).await.unwrap();
        assert_eq!(result.description.as_deref(), Some("Summary"));
    }

    #[test]
    fn invalid_params_error_uses_mcp_invalid_params_code() {
        let error = invalid_params_error("bad prompt".to_string());

        assert_eq!(error.code, rmcp::model::ErrorCode::INVALID_PARAMS);
        assert!(error.message.contains("bad prompt"));
    }
}
