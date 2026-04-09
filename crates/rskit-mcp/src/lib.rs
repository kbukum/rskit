//! Bridge between rskit tool registry and Model Context Protocol (MCP).
//!
//! This crate connects rskit's tool system to the MCP protocol using the
//! official Rust MCP SDK (`rmcp`). It provides:
//!
//! - **Server**: expose an rskit [`Registry`](rskit_tool::Registry) as an MCP server
//! - **Client**: connect to an MCP server and wrap remote tools as rskit [`Callable`](rskit_tool::Callable)
//! - **Convert**: bidirectional type conversions between rskit and MCP types

pub mod convert;

#[cfg(feature = "server")]
pub mod server;

#[cfg(feature = "client")]
pub mod client;

// Re-export key types for convenience
pub use convert::{
    app_error_to_mcp_error, call_result_to_tool_result, definition_to_tool,
    definitions_to_list_result, tool_result_to_call_result, tool_to_definition,
};

#[cfg(feature = "server")]
pub use server::{RegistryHandler, ServerConfig, create_server};

#[cfg(feature = "client")]
pub use client::{ClientConfig, discover_tools, wrap_tools};

// Re-export rmcp for downstream convenience
pub use rmcp;
