//! MCP server configuration.

use std::sync::Arc;

use crate::audit::ToolAuditSink;
use crate::authz::ToolAuthorizer;
use crate::prompts::PromptEntry;
use crate::resources::{ResourceEntry, ResourceTemplateEntry};

/// Configuration for the MCP server.
#[derive(Clone, Default)]
pub struct ServerConfig {
    /// Optional prefix prepended to all tool names exposed via MCP.
    pub prefix: String,
    /// Optional registry tool-name allow-list. Empty means expose all registered tools.
    pub allowed_tools: Vec<String>,
    /// Optional per-call authorization hook.
    pub tool_authorizer: Option<Arc<dyn ToolAuthorizer>>,
    /// Optional audit sink for all tool invocation outcomes.
    pub tool_audit_sink: Option<Arc<dyn ToolAuditSink>>,
    /// Reject JSON argument payloads larger than this many bytes. Zero disables the limit.
    pub max_input_bytes: usize,
    /// Reject serialized tool results larger than this many bytes. Zero disables the limit.
    pub max_result_bytes: usize,
    /// Static MCP prompts exposed by this server.
    pub prompts: Vec<PromptEntry>,
    /// Static MCP resources exposed by this server.
    pub resources: Vec<ResourceEntry>,
    /// Static MCP resource templates exposed by this server.
    pub resource_templates: Vec<ResourceTemplateEntry>,
}
