//! Bridge between rskit tool registry and Model Context Protocol (MCP).
//!
//! This crate connects rskit's tool system to the MCP protocol using the
//! official Rust MCP SDK (`rmcp`). It provides:
//!
//! - **Server**: expose an rskit [`Registry`](rskit_tool::Registry) as an MCP server
//! - **Client**: connect to an MCP server and wrap remote tools as rskit [`Callable`](rskit_tool::Callable)
//! - **Convert**: bidirectional type conversions between rskit and MCP types
//! - **Skill integration**: consumes top-level `rskit-skill`; remote MCP servers are tool sources
//! - **Transport helpers**: validate canonical transport names and build localhost-only
//!   Streamable HTTP configuration defaults

#[cfg(feature = "protocol")]
pub mod convert;
#[cfg(feature = "server")]
pub mod transport;

#[cfg(feature = "server")]
pub mod audit;
#[cfg(feature = "server")]
pub mod authorizer;
#[cfg(feature = "server")]
pub mod authz;
#[cfg(feature = "server")]
pub mod config;
#[cfg(feature = "server")]
pub mod limits;
#[cfg(feature = "server")]
pub mod prompts;
#[cfg(feature = "server")]
pub mod resources;
#[cfg(feature = "server")]
pub mod server;
#[cfg(feature = "server")]
pub mod tool_call;

#[cfg(feature = "client")]
pub mod client;

#[cfg(feature = "protocol")]
pub use convert::{
    app_error_to_mcp_error, call_result_to_tool_result, definition_to_tool,
    definitions_to_list_result, tool_result_to_call_result, tool_to_definition,
};

#[cfg(feature = "server")]
pub use audit::{ToolAuditEvent, ToolAuditSink};
#[cfg(feature = "server")]
pub use authorizer::DeciderToolAuthorizer;
#[cfg(feature = "server")]
pub use authz::{ToolAuthorizationDecision, ToolAuthorizationRequest, ToolAuthorizer};
#[cfg(feature = "server")]
pub use config::ServerConfig;
#[cfg(feature = "server")]
pub use prompts::PromptEntry;
#[cfg(feature = "server")]
pub use resources::{ResourceEntry, ResourceTemplateEntry};
#[cfg(feature = "server")]
pub use server::{RegistryHandler, create_server};
#[cfg(feature = "server")]
pub use transport::{
    TRANSPORT_STDIO, TRANSPORT_STREAMABLE_HTTP, TransportKind, streamable_http_server_config,
};

#[cfg(feature = "client")]
pub use client::{ClientConfig, discover_tools, wrap_tools};
