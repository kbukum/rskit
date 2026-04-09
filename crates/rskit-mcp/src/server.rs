//! MCP server backed by an rskit tool [`Registry`].
//!
//! Implements the MCP `ServerHandler` trait, delegating `tools/list` and
//! `tools/call` to the registry while providing sensible defaults for the
//! rest of the protocol.

use std::sync::Arc;

use rmcp::handler::server::ServerHandler;
use rmcp::model::{
    CallToolRequestParams, CallToolResult, Implementation, ListToolsResult, PaginatedRequestParams,
    ServerCapabilities, ServerInfo, Tool,
};
use rmcp::service::{RequestContext, RoleServer};
use tracing;

use rskit_tool::context::Context;
use rskit_tool::registry::Registry;

use crate::convert;

/// Configuration for the MCP server.
#[derive(Debug, Clone, Default)]
pub struct ServerConfig {
    /// Optional prefix prepended to all tool names exposed via MCP.
    pub prefix: String,
}

/// An MCP server handler backed by an rskit [`Registry`].
///
/// Created via [`create_server`].
pub struct RegistryHandler {
    name: String,
    version: String,
    registry: Arc<Registry>,
    config: ServerConfig,
}

impl RegistryHandler {
    fn mcp_tools(&self) -> Vec<Tool> {
        self.registry
            .list()
            .iter()
            .map(|d| convert::definition_to_tool(d, &self.config.prefix))
            .collect()
    }

    /// Strip the configured prefix from a tool name to recover the registry key.
    fn strip_prefix<'a>(&self, name: &'a str) -> &'a str {
        if !self.config.prefix.is_empty() && name.starts_with(&self.config.prefix) {
            &name[self.config.prefix.len()..]
        } else {
            name
        }
    }
}

impl ServerHandler for RegistryHandler {
    fn get_info(&self) -> ServerInfo {
        let capabilities = ServerCapabilities::builder().enable_tools().build();
        let server_info = Implementation::new(&self.name, &self.version);

        ServerInfo::new(capabilities)
            .with_server_info(server_info)
            .with_instructions(format!(
                "Tool server '{}' v{} — {} tools available",
                self.name,
                self.version,
                self.registry.len()
            ))
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, rmcp::ErrorData> {
        let tools = self.mcp_tools();
        tracing::debug!(count = tools.len(), "MCP tools/list");
        Ok(ListToolsResult {
            tools,
            next_cursor: None,
            meta: None,
        })
    }

    fn get_tool(&self, name: &str) -> Option<Tool> {
        let registry_name = self.strip_prefix(name);
        self.registry
            .get(registry_name)
            .map(|t| convert::definition_to_tool(t.definition(), &self.config.prefix))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        let tool_name = self.strip_prefix(&request.name);
        tracing::debug!(tool = tool_name, "MCP tools/call");

        let input = match request.arguments {
            Some(args) => serde_json::Value::Object(args),
            None => serde_json::Value::Object(serde_json::Map::new()),
        };

        let ctx = Context::new();

        match self.registry.call(tool_name, &ctx, input).await {
            Ok(result) => Ok(convert::tool_result_to_call_result(&result)),
            Err(err) => {
                tracing::warn!(tool = tool_name, error = %err, "tool call failed");
                Ok(CallToolResult::error(vec![rmcp::model::Content::text(
                    err.message.clone(),
                )]))
            }
        }
    }
}

/// Create an MCP [`ServerHandler`] backed by an rskit [`Registry`].
///
/// # Arguments
///
/// * `name` — server name advertised in `initialize` response
/// * `version` — server version advertised in `initialize` response
/// * `registry` — the tool registry to expose
/// * `config` — optional server configuration (prefix, etc.)
pub fn create_server(
    name: impl Into<String>,
    version: impl Into<String>,
    registry: Arc<Registry>,
    config: ServerConfig,
) -> RegistryHandler {
    RegistryHandler {
        name: name.into(),
        version: version.into(),
        registry,
        config,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rskit_tool::{from_fn, text_result};
    use schemars::JsonSchema;
    use serde::Deserialize;

    #[derive(Deserialize, JsonSchema)]
    struct EchoInput {
        message: String,
    }

    fn test_registry() -> Arc<Registry> {
        let registry = Registry::new();
        registry
            .register(from_fn(
                "echo",
                "Echo a message back",
                |_ctx: Context, input: EchoInput| async move { Ok(text_result(&input.message)) },
            ))
            .unwrap();
        Arc::new(registry)
    }

    #[test]
    fn test_get_info() {
        let handler = create_server("test-server", "0.1.0", test_registry(), Default::default());
        let info = handler.get_info();
        assert_eq!(info.server_info.name, "test-server");
        assert_eq!(info.server_info.version, "0.1.0");
    }

    #[test]
    fn test_get_tool_found() {
        let handler = create_server("test", "0.1.0", test_registry(), Default::default());
        let tool = handler.get_tool("echo");
        assert!(tool.is_some());
        assert_eq!(tool.unwrap().name.as_ref(), "echo");
    }

    #[test]
    fn test_get_tool_not_found() {
        let handler = create_server("test", "0.1.0", test_registry(), Default::default());
        assert!(handler.get_tool("nonexistent").is_none());
    }

    #[test]
    fn test_get_tool_with_prefix() {
        let config = ServerConfig {
            prefix: "myapp_".to_string(),
        };
        let handler = create_server("test", "0.1.0", test_registry(), config);
        let tool = handler.get_tool("myapp_echo");
        assert!(tool.is_some());
        assert_eq!(tool.unwrap().name.as_ref(), "myapp_echo");
    }

    #[test]
    fn test_mcp_tools_lists_all() {
        let handler = create_server("test", "0.1.0", test_registry(), Default::default());
        let tools = handler.mcp_tools();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name.as_ref(), "echo");
    }

    #[test]
    fn test_mcp_tools_with_prefix() {
        let config = ServerConfig {
            prefix: "pre_".to_string(),
        };
        let handler = create_server("test", "0.1.0", test_registry(), config);
        let tools = handler.mcp_tools();
        assert_eq!(tools[0].name.as_ref(), "pre_echo");
    }
}
