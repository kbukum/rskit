//! MCP tool-call audit events and sinks.

use async_trait::async_trait;

/// Final audit event for an MCP tool invocation.
#[derive(Debug, Clone)]
pub struct ToolAuditEvent {
    /// Registry tool name.
    pub tool_name: String,
    /// Exposed MCP tool name.
    pub mcp_name: String,
    /// Final outcome.
    pub outcome: String,
    /// Decision or policy reason.
    pub reason: String,
    /// Error text, when present.
    pub error: String,
}

/// Sink that receives MCP tool audit events.
#[async_trait]
pub trait ToolAuditSink: Send + Sync {
    /// Record the final tool invocation event.
    async fn record_tool_call(&self, event: ToolAuditEvent);
}
