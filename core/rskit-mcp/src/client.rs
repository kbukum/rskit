//! MCP client that wraps remote tools as rskit [`Callable`] instances.
//!
//! Connects to an MCP server, discovers its tools, and wraps each one
//! as a [`Callable`] so they can be registered in an rskit [`rskit_tool::Registry`].

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use rmcp::model::{CallToolRequestParams, Tool};
use rmcp::service::{Peer, RoleClient, RunningService};
use rskit_ai::semconv;
use rskit_errors::{AppError, AppResult, ErrorCode};
use rskit_observability::set_span_attribute;
use rskit_schema::{CompiledSchema, ValidationResult};
use rskit_tool::context::Context;
use rskit_tool::result::ToolResult;
use rskit_tool::{Callable, Definition, ToolInput};
use tracing::Instrument;

use crate::convert;

/// Default timeout applied to each remote MCP request.
const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(30);

/// Configuration for the MCP client.
#[derive(Debug, Clone)]
pub struct ClientConfig {
    /// Optional prefix to strip from remote tool names.
    pub prefix: String,
    /// Timeout applied to each remote MCP request (`tools/call`, `tools/list`).
    ///
    /// A remote server that never responds must not block the caller
    /// indefinitely; every remote call is bounded by this deadline.
    pub request_timeout: Duration,
}

impl Default for ClientConfig {
    fn default() -> Self {
        Self {
            prefix: String::new(),
            request_timeout: DEFAULT_REQUEST_TIMEOUT,
        }
    }
}

impl ClientConfig {
    /// Set the per-request timeout for remote MCP calls.
    #[must_use]
    pub fn with_request_timeout(mut self, timeout: Duration) -> Self {
        self.request_timeout = timeout;
        self
    }
}

/// A remote MCP tool wrapped as an rskit [`Callable`].
///
/// Delegates `call()` to the MCP session's `call_tool` method.
struct RemoteTool {
    definition: Definition,
    input_validator: Result<CompiledSchema, ValidationResult>,
    mcp_name: String,
    peer: Arc<Peer<RoleClient>>,
    request_timeout: Duration,
}

#[async_trait]
impl Callable for RemoteTool {
    fn definition(&self) -> &Definition {
        &self.definition
    }

    fn validate(&self, input: &ToolInput) -> ValidationResult {
        match &self.input_validator {
            Ok(validator) => validator.validate(input.as_json()),
            Err(result) => result.clone(),
        }
    }

    async fn call(&self, _ctx: &Context, input: ToolInput) -> AppResult<ToolResult> {
        let span = tracing::info_span!(
            "mcp.request",
            "gen_ai.operation.name" = semconv::Operation::McpRequest.as_str(),
            "gen_ai.tool.name" = self.definition.name.as_str(),
            "mcp.method" = "tools/call",
            "mcp.tool_name" = self.mcp_name.as_str(),
        );
        set_span_attribute(
            &span,
            semconv::OPERATION_NAME,
            semconv::Operation::McpRequest.as_str(),
        );
        set_span_attribute(&span, semconv::TOOL_NAME, self.definition.name.as_str());
        async {
            let arguments = match input.into_json() {
                serde_json::Value::Object(map) => Some(map),
                _ => None,
            };

            let mut params = CallToolRequestParams::new(self.mcp_name.clone());
            if let Some(args) = arguments {
                params = params.with_arguments(args);
            }

            let result = tokio::time::timeout(self.request_timeout, self.peer.call_tool(params))
                .await
                .map_err(|_| {
                    AppError::new(
                        ErrorCode::Timeout,
                        format!("MCP call_tool timed out after {:?}", self.request_timeout),
                    )
                })?
                .map_err(|e| {
                    AppError::new(
                        ErrorCode::ExternalService,
                        format!("MCP call_tool failed: {e}"),
                    )
                })?;

            Ok(convert::call_result_to_tool_result(&result))
        }
        .instrument(span)
        .await
    }
}

/// Wrap discovered MCP tools as rskit [`Callable`] instances.
///
/// Takes the tools list from an MCP server and the peer handle, returns
/// boxed `Callable`s ready for registration in a [`rskit_tool::Registry`].
pub fn wrap_tools(
    tools: &[Tool],
    peer: Arc<Peer<RoleClient>>,
    config: &ClientConfig,
) -> AppResult<Vec<Box<dyn Callable>>> {
    tools
        .iter()
        .map(|tool| {
            let def = convert::tool_to_definition(tool, &config.prefix)?;
            let input_validator = rskit_schema::compile(def.input_schema.as_json())
                .map_err(validation_result_from_error);
            let mcp_name = tool.name.to_string();
            Ok(Box::new(RemoteTool {
                definition: def,
                input_validator,
                mcp_name,
                peer: peer.clone(),
                request_timeout: config.request_timeout,
            }) as Box<dyn Callable>)
        })
        .collect()
}

fn validation_result_from_error(err: AppError) -> ValidationResult {
    ValidationResult {
        valid: false,
        errors: vec![rskit_schema::ValidationError {
            path: String::new(),
            message: err.message().to_owned(),
        }],
    }
}

/// Connect to an MCP server, discover tools, and return them as [`Callable`]s.
///
/// This is a convenience function that:
/// 1. Takes an already-running client service
/// 2. Lists the server's tools
/// 3. Wraps each one as a `Callable`
///
/// The caller is responsible for creating the transport and starting the service
/// via `rmcp::serve_client`.
///
/// # Returns
///
/// A vector of boxed [`Callable`] instances, one per remote tool.
pub async fn discover_tools<S>(
    client: &RunningService<RoleClient, S>,
    config: &ClientConfig,
) -> AppResult<Vec<Box<dyn Callable>>>
where
    S: rmcp::service::Service<RoleClient>,
{
    let result = tokio::time::timeout(config.request_timeout, client.list_tools(None))
        .await
        .map_err(|_| {
            AppError::new(
                ErrorCode::Timeout,
                format!(
                    "MCP list_tools timed out after {:?}",
                    config.request_timeout
                ),
            )
        })?
        .map_err(|e| {
            AppError::new(
                ErrorCode::ExternalService,
                format!("MCP list_tools failed: {e}"),
            )
        })?;

    let peer = Arc::new(client.peer().clone());
    wrap_tools(&result.tools, peer, config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_config_default() {
        let config = ClientConfig::default();
        assert!(config.prefix.is_empty());
        assert_eq!(config.request_timeout, DEFAULT_REQUEST_TIMEOUT);
    }

    #[test]
    fn test_client_config_with_request_timeout() {
        let config = ClientConfig::default().with_request_timeout(Duration::from_secs(5));
        assert_eq!(config.request_timeout, Duration::from_secs(5));
    }
}
