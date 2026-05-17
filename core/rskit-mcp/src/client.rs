//! MCP client that wraps remote tools as rskit [`Callable`] instances.
//!
//! Connects to an MCP server, discovers its tools, and wraps each one
//! as a [`Callable`] so they can be registered in an rskit [`rskit_tool::Registry`].

use std::sync::Arc;

use async_trait::async_trait;
use rmcp::model::{CallToolRequestParams, Tool};
use rmcp::service::{Peer, RoleClient, RunningService};
use rskit_ai::semconv;
use rskit_errors::{AppError, AppResult, ErrorCode};
use rskit_tool::Callable;
use rskit_tool::Definition;
use rskit_tool::context::Context;
use rskit_tool::result::ToolResult;
use rskit_validation::{CompiledSchema, ValidationResult};
use tracing::Instrument;

use crate::convert;

/// Configuration for the MCP client.
#[derive(Debug, Clone, Default)]
pub struct ClientConfig {
    /// Optional prefix to strip from remote tool names.
    pub prefix: String,
}

/// A remote MCP tool wrapped as an rskit [`Callable`].
///
/// Delegates `call()` to the MCP session's `call_tool` method.
struct RemoteTool {
    definition: Definition,
    input_validator: Result<CompiledSchema, ValidationResult>,
    mcp_name: String,
    peer: Arc<Peer<RoleClient>>,
}

#[async_trait]
impl Callable for RemoteTool {
    fn definition(&self) -> &Definition {
        &self.definition
    }

    fn validate(&self, input: &serde_json::Value) -> ValidationResult {
        match &self.input_validator {
            Ok(validator) => validator.validate(input),
            Err(result) => result.clone(),
        }
    }

    async fn call(&self, _ctx: &Context, input: serde_json::Value) -> AppResult<ToolResult> {
        let span = tracing::info_span!(
            "mcp.request",
            "gen_ai.operation.name" = semconv::Operation::McpRequest.as_str(),
            "gen_ai.tool.name" = self.definition.name.as_str(),
            "mcp.method" = "tools/call",
            "mcp.tool_name" = self.mcp_name.as_str(),
        );
        async {
            let arguments = match input {
                serde_json::Value::Object(map) => Some(map),
                _ => None,
            };

            let mut params = CallToolRequestParams::new(self.mcp_name.clone());
            if let Some(args) = arguments {
                params = params.with_arguments(args);
            }

            let result = self.peer.call_tool(params).await.map_err(|e| {
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
) -> Vec<Box<dyn Callable>> {
    tools
        .iter()
        .map(|tool| {
            let def = convert::tool_to_definition(tool, &config.prefix);
            let input_validator =
                rskit_validation::compile(&def.input_schema).map_err(validation_result_from_error);
            let mcp_name = tool.name.to_string();
            Box::new(RemoteTool {
                definition: def,
                input_validator,
                mcp_name,
                peer: peer.clone(),
            }) as Box<dyn Callable>
        })
        .collect()
}

fn validation_result_from_error(err: AppError) -> ValidationResult {
    ValidationResult {
        valid: false,
        errors: vec![rskit_validation::ValidationError {
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
    let result = client.list_tools(None).await.map_err(|e| {
        AppError::new(
            ErrorCode::ExternalService,
            format!("MCP list_tools failed: {e}"),
        )
    })?;

    let peer = Arc::new(client.peer().clone());
    Ok(wrap_tools(&result.tools, peer, config))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_client_config_default() {
        let config = ClientConfig::default();
        assert!(config.prefix.is_empty());
    }
}
