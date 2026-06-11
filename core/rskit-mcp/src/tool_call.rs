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
}
