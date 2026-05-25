//! MCP tool-call authorization contracts.

use async_trait::async_trait;
use rskit_tool::ToolInput;

/// MCP tool authorization input.
#[derive(Debug, Clone)]
pub struct ToolAuthorizationRequest {
    /// Registry tool name.
    pub tool_name: String,
    /// Exposed MCP tool name.
    pub mcp_name: String,
    /// Validated invocation arguments.
    pub arguments: ToolInput,
}

/// MCP tool authorization decision.
#[derive(Debug, Clone)]
pub struct ToolAuthorizationDecision {
    /// Whether the call is allowed.
    pub allowed: bool,
    /// Human-readable reason.
    pub reason: String,
}

/// Per-call MCP tool authorizer.
#[async_trait]
pub trait ToolAuthorizer: Send + Sync {
    /// Evaluate the tool invocation before execution.
    async fn authorize_tool(
        &self,
        request: &ToolAuthorizationRequest,
    ) -> Result<ToolAuthorizationDecision, String>;
}
