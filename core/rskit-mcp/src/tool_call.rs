//! MCP `tools/call` policy and execution workflow.

use rmcp::model::{CallToolRequestParams, CallToolResult};
use rskit_ai::semconv;
use rskit_observability::set_span_attribute;
use rskit_tool::ToolInput;
use rskit_tool::context::Context;
use tracing::Instrument;

use crate::audit::ToolAuditEvent;
use crate::authz::{ToolAuthorizationDecision, ToolAuthorizationRequest};
use crate::convert;
use crate::limits::{
    json_size_bytes, result_size_bytes, validate_tool_output, validation_error_details,
};
use crate::server::RegistryHandler;

impl RegistryHandler {
    /// Strip the configured prefix from a tool name to recover the registry key.
    pub(crate) fn strip_prefix<'a>(&self, name: &'a str) -> &'a str {
        if !self.config.prefix.is_empty() && name.starts_with(&self.config.prefix) {
            &name[self.config.prefix.len()..]
        } else {
            name
        }
    }

    pub(crate) fn allows_tool(&self, name: &str) -> bool {
        self.config.allowed_tools.is_empty()
            || self
                .config
                .allowed_tools
                .iter()
                .any(|allowed| allowed == name)
    }

    pub(crate) async fn handle_call_tool(&self, request: CallToolRequestParams) -> CallToolResult {
        let tool_name = self.strip_prefix(&request.name);
        let span = tracing::info_span!(
            "mcp.request",
            "gen_ai.operation.name" = semconv::Operation::McpRequest.as_str(),
            "gen_ai.tool.name" = tool_name,
            "mcp.method" = "tools/call",
            "mcp.tool_name" = %request.name,
        );
        set_span_attribute(
            &span,
            semconv::OPERATION_NAME,
            semconv::Operation::McpRequest.as_str(),
        );
        set_span_attribute(&span, semconv::TOOL_NAME, tool_name);
        async {
            tracing::debug!(tool = tool_name, "MCP tools/call");

            let mut event = ToolAuditEvent {
                tool_name: tool_name.to_string(),
                mcp_name: request.name.to_string(),
                outcome: String::new(),
                reason: String::new(),
                error: String::new(),
            };

            if !self.allows_tool(tool_name) {
                event.outcome = String::from("denied");
                event.reason = String::from("not in allow-list");
                self.audit_tool_call(event).await;
                tracing::warn!(tool = tool_name, "MCP tool call rejected by allow-list");
                return CallToolResult::error(vec![rmcp::model::Content::text(
                    "tool is not allowed",
                )]);
            }

            let input = ToolInput::from_object(request.arguments.unwrap_or_default());

            if self.config.max_input_bytes > 0
                && json_size_bytes(input.as_json()) > self.config.max_input_bytes
            {
                event.outcome = String::from("input_too_large");
                event.error = format!("input size exceeds {} bytes", self.config.max_input_bytes);
                self.audit_tool_call(event).await;
                return CallToolResult::error(vec![rmcp::model::Content::text(format!(
                    "input too large: exceeds {} bytes",
                    self.config.max_input_bytes
                ))]);
            }

            let tool = match self.registry.get(tool_name) {
                Some(tool) => tool,
                None => {
                    event.outcome = String::from("not_found");
                    event.error = format!("tool not found: {tool_name}");
                    self.audit_tool_call(event).await;
                    return CallToolResult::error(vec![rmcp::model::Content::text(format!(
                        "tool not found: {tool_name}"
                    ))]);
                }
            };

            let validation = tool.validate(&input);
            if !validation.valid {
                let details = validation_error_details(&validation);
                event.outcome = String::from("invalid_input");
                event.error = details.clone();
                self.audit_tool_call(event).await;
                return CallToolResult::error(vec![rmcp::model::Content::text(format!(
                    "invalid tool input: {details}"
                ))]);
            }

            let tool_def = tool.definition().clone();

            match self
                .authorize_tool_call(ToolAuthorizationRequest {
                    tool_name: tool_name.to_string(),
                    mcp_name: request.name.to_string(),
                    arguments: input.clone(),
                })
                .await
            {
                Ok(decision) => {
                    event.reason = decision.reason.clone();
                    if !decision.allowed {
                        event.outcome = String::from("denied");
                        self.audit_tool_call(event).await;
                        return CallToolResult::error(vec![rmcp::model::Content::text(
                            denied_message(&decision.reason),
                        )]);
                    }
                }
                Err(err) => {
                    event.outcome = String::from("authorization_error");
                    event.error = err.clone();
                    self.audit_tool_call(event).await;
                    return CallToolResult::error(vec![rmcp::model::Content::text(
                        "authorization error",
                    )]);
                }
            }

            let ctx = Context::new();

            let result = match self.registry.call_validated(tool_name, &ctx, input).await {
                Ok(result) => {
                    if result.is_error {
                        event.outcome = String::from("tool_error");
                        event.error = result.text().to_string();
                    } else {
                        let limit = self.config.max_result_bytes;
                        if limit > 0 && result_size_bytes(&result) > limit {
                            event.outcome = String::from("result_too_large");
                            event.error = format!("result size exceeds {limit} bytes");
                            self.audit_tool_call(event).await;
                            return CallToolResult::error(vec![rmcp::model::Content::text(
                                format!("result too large: exceeds {limit} bytes"),
                            )]);
                        }
                        if let Some(message) = validate_tool_output(&tool_def, &result) {
                            event.outcome = String::from("output_validation_error");
                            event.error = message.clone();
                            self.audit_tool_call(event).await;
                            return CallToolResult::error(vec![rmcp::model::Content::text(
                                format!("output validation error: {message}"),
                            )]);
                        }
                        event.outcome = String::from("success");
                    }
                    convert::tool_result_to_call_result(&result)
                }
                Err(err) => {
                    event.outcome = String::from("tool_error");
                    event.error = err.message().to_string();
                    tracing::warn!(tool = tool_name, error = %err, "tool call failed");
                    CallToolResult::error(vec![rmcp::model::Content::text(
                        err.message().to_string(),
                    )])
                }
            };

            self.audit_tool_call(event).await;
            result
        }
        .instrument(span)
        .await
    }

    async fn authorize_tool_call(
        &self,
        request: ToolAuthorizationRequest,
    ) -> Result<ToolAuthorizationDecision, String> {
        match &self.config.tool_authorizer {
            Some(authorizer) => authorizer.authorize_tool(&request).await,
            None => Ok(ToolAuthorizationDecision {
                allowed: true,
                reason: String::from("no_authorizer"),
            }),
        }
    }

    async fn audit_tool_call(&self, event: ToolAuditEvent) {
        if let Some(sink) = &self.config.tool_audit_sink {
            sink.record_tool_call(event).await;
        }
    }
}

fn denied_message(reason: &str) -> String {
    if reason.is_empty() {
        String::from("tool call denied")
    } else {
        format!("tool call denied: {reason}")
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use rskit_tool::Registry;

    use crate::config::ServerConfig;
    use crate::server::create_server;

    use super::*;

    #[test]
    fn prefix_and_allow_list_helpers_are_exact() {
        let handler = create_server(
            "test",
            "0.1.0",
            Arc::new(Registry::new()),
            ServerConfig {
                prefix: "pre_".to_string(),
                allowed_tools: vec!["echo".to_string()],
                ..Default::default()
            },
        );

        assert_eq!(handler.strip_prefix("pre_echo"), "echo");
        assert_eq!(handler.strip_prefix("other_echo"), "other_echo");
        assert!(handler.allows_tool("echo"));
        assert!(!handler.allows_tool("missing"));

        let open = create_server(
            "test",
            "0.1.0",
            Arc::new(Registry::new()),
            Default::default(),
        );
        assert_eq!(open.strip_prefix("echo"), "echo");
        assert!(open.allows_tool("anything"));
    }

    #[test]
    fn denied_message_omits_empty_reason() {
        assert_eq!(denied_message(""), "tool call denied");
        assert_eq!(denied_message("policy"), "tool call denied: policy");
    }

    use async_trait::async_trait;
    use parking_lot::Mutex;
    use rmcp::model::{CallToolRequestParams, CallToolResult};
    use rskit_errors::{AppError, ErrorCode};
    use rskit_tool::context::Context;
    use rskit_tool::{ToolMetadata, ToolResult, from_fn, text_result};
    use schemars::JsonSchema;
    use serde::Deserialize;
    use serde_json::json;

    use crate::audit::{ToolAuditEvent, ToolAuditSink};
    use crate::authz::{ToolAuthorizationDecision, ToolAuthorizationRequest, ToolAuthorizer};

    #[derive(Deserialize, JsonSchema)]
    struct EchoInput {
        message: String,
    }

    fn echo_registry() -> Arc<Registry> {
        let registry = Registry::new();
        registry
            .register(
                from_fn(
                    "echo",
                    "Echo a message back",
                    |_ctx: Context, input: EchoInput| async move { Ok(text_result(&input.message)) },
                )
                .unwrap(),
            )
            .unwrap();
        Arc::new(registry)
    }

    fn failing_registry() -> Arc<Registry> {
        let registry = Registry::new();
        registry
            .register(
                from_fn(
                    "boom",
                    "Always fails",
                    |_ctx: Context, _input: EchoInput| async move {
                        Err(AppError::new(ErrorCode::Internal, "tool exploded"))
                    },
                )
                .unwrap(),
            )
            .unwrap();
        Arc::new(registry)
    }

    struct ErroringAuthorizer;

    #[async_trait]
    impl ToolAuthorizer for ErroringAuthorizer {
        async fn authorize_tool(
            &self,
            _request: &ToolAuthorizationRequest,
        ) -> Result<ToolAuthorizationDecision, String> {
            Err("authorizer backend unavailable".to_string())
        }
    }

    #[derive(Default)]
    struct RecordingAuditSink {
        events: Arc<Mutex<Vec<ToolAuditEvent>>>,
    }

    #[async_trait]
    impl ToolAuditSink for RecordingAuditSink {
        async fn record_tool_call(&self, event: ToolAuditEvent) {
            self.events.lock().push(event);
        }
    }

    fn first_text(result: &CallToolResult) -> Option<&str> {
        result
            .content
            .first()
            .and_then(|content| match &content.raw {
                rmcp::model::RawContent::Text(text) => Some(text.text.as_ref()),
                _ => None,
            })
    }

    fn echo_request() -> CallToolRequestParams {
        serde_json::from_value(json!({
            "name": "echo",
            "arguments": { "message": "hello" }
        }))
        .unwrap()
    }

    #[tokio::test]
    async fn call_time_allow_list_denies_tool_outside_list() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let config = ServerConfig {
            allowed_tools: vec!["other".to_string()],
            tool_audit_sink: Some(Arc::new(RecordingAuditSink {
                events: Arc::clone(&events),
            })),
            ..Default::default()
        };
        let handler = create_server("test", "0.1.0", echo_registry(), config);

        let result = handler.handle_call_tool(echo_request()).await;

        assert_eq!(result.is_error, Some(true));
        assert_eq!(first_text(&result), Some("tool is not allowed"));
        let captured = events.lock();
        assert_eq!(captured[0].outcome, "denied");
        assert_eq!(captured[0].reason, "not in allow-list");
    }

    #[tokio::test]
    async fn oversized_result_is_rejected_before_returning_to_caller() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let config = ServerConfig {
            max_result_bytes: 4,
            tool_audit_sink: Some(Arc::new(RecordingAuditSink {
                events: Arc::clone(&events),
            })),
            ..Default::default()
        };
        let handler = create_server("test", "0.1.0", echo_registry(), config);

        let result = handler.handle_call_tool(echo_request()).await;

        assert_eq!(result.is_error, Some(true));
        assert_eq!(
            first_text(&result),
            Some("result too large: exceeds 4 bytes")
        );
        assert_eq!(events.lock()[0].outcome, "result_too_large");
    }

    #[tokio::test]
    async fn authorizer_backend_error_fails_closed_without_leaking_detail() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let config = ServerConfig {
            tool_authorizer: Some(Arc::new(ErroringAuthorizer)),
            tool_audit_sink: Some(Arc::new(RecordingAuditSink {
                events: Arc::clone(&events),
            })),
            ..Default::default()
        };
        let handler = create_server("test", "0.1.0", echo_registry(), config);

        let result = handler.handle_call_tool(echo_request()).await;

        assert_eq!(result.is_error, Some(true));
        // Caller sees a generic message; the backend detail stays in the audit trail only.
        assert_eq!(first_text(&result), Some("authorization error"));
        let captured = events.lock();
        assert_eq!(captured[0].outcome, "authorization_error");
        assert_eq!(captured[0].error, "authorizer backend unavailable");
    }

    #[tokio::test]
    async fn tool_execution_failure_is_reported_and_audited() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let config = ServerConfig {
            tool_audit_sink: Some(Arc::new(RecordingAuditSink {
                events: Arc::clone(&events),
            })),
            ..Default::default()
        };
        let handler = create_server("test", "0.1.0", failing_registry(), config);

        let request: CallToolRequestParams = serde_json::from_value(json!({
            "name": "boom",
            "arguments": { "message": "hi" }
        }))
        .unwrap();
        let result = handler.handle_call_tool(request).await;

        assert_eq!(result.is_error, Some(true));
        assert_eq!(first_text(&result), Some("tool exploded"));
        assert_eq!(events.lock()[0].outcome, "tool_error");
    }

    #[tokio::test]
    async fn tool_returning_error_result_is_audited_as_tool_error() {
        use rskit_schema::ValidationResult;
        use rskit_tool::{Callable, Definition};

        struct ErrorResultTool {
            definition: Definition,
        }

        #[async_trait]
        impl Callable for ErrorResultTool {
            fn definition(&self) -> &Definition {
                &self.definition
            }

            fn validate(&self, _input: &ToolInput) -> ValidationResult {
                ValidationResult {
                    valid: true,
                    errors: Vec::new(),
                }
            }

            async fn call(
                &self,
                _ctx: &Context,
                _input: ToolInput,
            ) -> rskit_errors::AppResult<ToolResult> {
                Ok(ToolResult {
                    output: None,
                    content: String::from("boom from tool body"),
                    is_error: true,
                    metadata: ToolMetadata::new(),
                })
            }
        }

        let registry = Registry::new();
        registry
            .register(Box::new(ErrorResultTool {
                definition: Definition {
                    name: String::from("err"),
                    description: String::from("Returns an error result"),
                    input_schema: rskit_tool::ToolSchema::new(json!({
                        "type": "object", "properties": {}
                    }))
                    .unwrap(),
                    output_schema: None,
                    annotations: rskit_tool::Annotations::default(),
                    envelope: rskit_tool::Envelope::default(),
                },
            }))
            .unwrap();

        let events = Arc::new(Mutex::new(Vec::new()));
        let config = ServerConfig {
            tool_audit_sink: Some(Arc::new(RecordingAuditSink {
                events: Arc::clone(&events),
            })),
            ..Default::default()
        };
        let handler = create_server("test", "0.1.0", Arc::new(registry), config);

        let request: CallToolRequestParams = serde_json::from_value(json!({
            "name": "err",
            "arguments": {}
        }))
        .unwrap();
        let result = handler.handle_call_tool(request).await;

        assert_eq!(result.is_error, Some(true));
        let captured = events.lock();
        assert_eq!(captured[0].outcome, "tool_error");
        assert_eq!(captured[0].error, "boom from tool body");
    }
}
