//! MCP server backed by an rskit tool [`Registry`].
//!
//! Implements the MCP `ServerHandler` trait, delegating `tools/list` and
//! `tools/call` to the registry while providing sensible defaults for the
//! rest of the protocol.

use std::{future::Future, pin::Pin, sync::Arc};

use async_trait::async_trait;
use rmcp::handler::server::ServerHandler;
use rmcp::model::{
    CallToolRequestParams, CallToolResult, GetPromptRequestParams, GetPromptResult, Implementation,
    ListPromptsResult, ListResourceTemplatesResult, ListResourcesResult, ListToolsResult,
    PaginatedRequestParams, Prompt, ReadResourceRequestParams, ReadResourceResult, Resource,
    ResourceTemplate, ServerCapabilities, ServerInfo, Tool,
};
use rmcp::service::{RequestContext, RoleServer};
use rskit_ai::semconv;
use rskit_component::{Component, Health};
use tracing;
use tracing::Instrument;

use rskit_tool::ToolInput;
use rskit_tool::context::Context;
use rskit_tool::registry::Registry;

use crate::convert;

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

type PromptFuture = Pin<Box<dyn Future<Output = Result<GetPromptResult, rmcp::ErrorData>> + Send>>;
type ResourceFuture =
    Pin<Box<dyn Future<Output = Result<ReadResourceResult, rmcp::ErrorData>> + Send>>;

/// Static MCP prompt registration.
pub struct PromptEntry {
    /// Prompt metadata exposed to clients.
    pub prompt: Prompt,
    handler: Arc<dyn Fn(GetPromptRequestParams) -> PromptFuture + Send + Sync>,
}

impl PromptEntry {
    /// Construct a prompt entry from prompt metadata and an async handler.
    pub fn new<F, Fut>(prompt: Prompt, handler: F) -> Self
    where
        F: Fn(GetPromptRequestParams) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<GetPromptResult, rmcp::ErrorData>> + Send + 'static,
    {
        Self {
            prompt,
            handler: Arc::new(move |request| Box::pin(handler(request))),
        }
    }
}

impl Clone for PromptEntry {
    fn clone(&self) -> Self {
        Self {
            prompt: self.prompt.clone(),
            handler: Arc::clone(&self.handler),
        }
    }
}

/// Static MCP resource registration.
pub struct ResourceEntry {
    /// Resource metadata exposed to clients.
    pub resource: Resource,
    handler: Arc<dyn Fn(ReadResourceRequestParams) -> ResourceFuture + Send + Sync>,
}

impl ResourceEntry {
    /// Construct a resource entry from resource metadata and an async handler.
    pub fn new<F, Fut>(resource: Resource, handler: F) -> Self
    where
        F: Fn(ReadResourceRequestParams) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<ReadResourceResult, rmcp::ErrorData>> + Send + 'static,
    {
        Self {
            resource,
            handler: Arc::new(move |request| Box::pin(handler(request))),
        }
    }
}

impl Clone for ResourceEntry {
    fn clone(&self) -> Self {
        Self {
            resource: self.resource.clone(),
            handler: Arc::clone(&self.handler),
        }
    }
}

/// Static MCP resource-template registration.
pub struct ResourceTemplateEntry {
    /// Resource-template metadata exposed to clients.
    pub resource_template: ResourceTemplate,
    handler: Arc<dyn Fn(ReadResourceRequestParams) -> ResourceFuture + Send + Sync>,
}

impl ResourceTemplateEntry {
    /// Construct a resource-template entry from metadata and an async handler.
    pub fn new<F, Fut>(resource_template: ResourceTemplate, handler: F) -> Self
    where
        F: Fn(ReadResourceRequestParams) -> Fut + Send + Sync + 'static,
        Fut: Future<Output = Result<ReadResourceResult, rmcp::ErrorData>> + Send + 'static,
    {
        Self {
            resource_template,
            handler: Arc::new(move |request| Box::pin(handler(request))),
        }
    }
}

impl Clone for ResourceTemplateEntry {
    fn clone(&self) -> Self {
        Self {
            resource_template: self.resource_template.clone(),
            handler: Arc::clone(&self.handler),
        }
    }
}

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
    pub(crate) fn mcp_tools(&self) -> Vec<Tool> {
        self.registry
            .list()
            .iter()
            .filter(|d| self.allows_tool(&d.name))
            .map(|d| convert::definition_to_tool(d, &self.config.prefix))
            .collect()
    }

    pub(crate) fn mcp_prompts(&self) -> Vec<Prompt> {
        self.config
            .prompts
            .iter()
            .map(|entry| entry.prompt.clone())
            .collect()
    }

    pub(crate) fn mcp_resources(&self) -> Vec<Resource> {
        self.config
            .resources
            .iter()
            .map(|entry| entry.resource.clone())
            .collect()
    }

    pub(crate) fn mcp_resource_templates(&self) -> Vec<ResourceTemplate> {
        self.config
            .resource_templates
            .iter()
            .map(|entry| entry.resource_template.clone())
            .collect()
    }

    pub(crate) async fn handle_get_prompt(
        &self,
        request: GetPromptRequestParams,
    ) -> Result<GetPromptResult, rmcp::ErrorData> {
        let entry = self
            .config
            .prompts
            .iter()
            .find(|entry| prompt_name(&entry.prompt).as_deref() == Some(request.name.as_str()))
            .ok_or_else(|| invalid_params_error(format!("prompt not found: {}", request.name)))?;
        (entry.handler)(request).await
    }

    pub(crate) async fn handle_read_resource(
        &self,
        request: ReadResourceRequestParams,
    ) -> Result<ReadResourceResult, rmcp::ErrorData> {
        let uri = request.uri.to_string();
        if let Some(entry) = self
            .config
            .resources
            .iter()
            .find(|entry| resource_uri(&entry.resource).as_deref() == Some(uri.as_str()))
        {
            return (entry.handler)(request).await;
        }
        if let Some(entry) = self.config.resource_templates.iter().find(|entry| {
            resource_template_uri(&entry.resource_template)
                .map(|template| resource_template_matches(&template, &uri))
                .unwrap_or(false)
        }) {
            return (entry.handler)(request).await;
        }
        Err(invalid_params_error(format!("resource not found: {uri}")))
    }

    /// Strip the configured prefix from a tool name to recover the registry key.
    fn strip_prefix<'a>(&self, name: &'a str) -> &'a str {
        if !self.config.prefix.is_empty() && name.starts_with(&self.config.prefix) {
            &name[self.config.prefix.len()..]
        } else {
            name
        }
    }

    fn allows_tool(&self, name: &str) -> bool {
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

            let input_value = match request.arguments {
                Some(args) => serde_json::Value::Object(args),
                None => serde_json::Value::Object(serde_json::Map::new()),
            };
            let input = match ToolInput::new(input_value) {
                Ok(input) => input,
                Err(error) => {
                    event.outcome = String::from("invalid_input");
                    event.error = error.to_string();
                    self.audit_tool_call(event).await;
                    return CallToolResult::error(vec![rmcp::model::Content::text(
                        "tool input must be a JSON object",
                    )]);
                }
            };

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

            let ctx = Context::new();
            let tool_def = self
                .registry
                .get(tool_name)
                .map(|tool| tool.definition().clone());

            let result = match self.registry.call(tool_name, &ctx, input).await {
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
                        if let Some(definition) = &tool_def
                            && let Some(message) = validate_tool_output(definition, &result)
                        {
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
                    event.error = err.message.clone();
                    tracing::warn!(tool = tool_name, error = %err, "tool call failed");
                    CallToolResult::error(vec![rmcp::model::Content::text(err.message.clone())])
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

impl ServerHandler for RegistryHandler {
    fn get_info(&self) -> ServerInfo {
        let capabilities = ServerCapabilities::builder()
            .enable_tools()
            .enable_prompts()
            .enable_resources()
            .build();
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

    async fn list_prompts(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListPromptsResult, rmcp::ErrorData> {
        Ok(ListPromptsResult {
            prompts: self.mcp_prompts(),
            ..Default::default()
        })
    }

    async fn get_prompt(
        &self,
        request: GetPromptRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<GetPromptResult, rmcp::ErrorData> {
        self.handle_get_prompt(request).await
    }

    async fn list_resources(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourcesResult, rmcp::ErrorData> {
        Ok(ListResourcesResult {
            resources: self.mcp_resources(),
            ..Default::default()
        })
    }

    async fn list_resource_templates(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListResourceTemplatesResult, rmcp::ErrorData> {
        Ok(ListResourceTemplatesResult {
            resource_templates: self.mcp_resource_templates(),
            ..Default::default()
        })
    }

    async fn read_resource(
        &self,
        request: ReadResourceRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<ReadResourceResult, rmcp::ErrorData> {
        self.handle_read_resource(request).await
    }

    fn get_tool(&self, name: &str) -> Option<Tool> {
        let registry_name = self.strip_prefix(name);
        if !self.allows_tool(registry_name) {
            return None;
        }
        self.registry
            .get(registry_name)
            .map(|t| convert::definition_to_tool(t.definition(), &self.config.prefix))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        _context: RequestContext<RoleServer>,
    ) -> Result<CallToolResult, rmcp::ErrorData> {
        Ok(self.handle_call_tool(request).await)
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

fn denied_message(reason: &str) -> String {
    if reason.is_empty() {
        String::from("tool call denied")
    } else {
        format!("tool call denied: {reason}")
    }
}

fn json_size_bytes(value: &serde_json::Value) -> usize {
    serde_json::to_vec(value).map_or(0, |bytes| bytes.len())
}

fn result_size_bytes(result: &rskit_tool::result::ToolResult) -> usize {
    if let Some(output) = &result.output {
        return json_size_bytes(output.as_json());
    }
    result.content.len()
}

fn validate_tool_output(
    definition: &rskit_tool::Definition,
    result: &rskit_tool::result::ToolResult,
) -> Option<String> {
    let schema = definition.output_schema.as_ref()?;
    if result.is_error {
        return None;
    }
    let candidate = result
        .output
        .clone()
        .map(rskit_tool::ToolOutput::into_json)
        .unwrap_or_else(|| serde_json::Value::String(result.content.clone()));
    let validation = rskit_schema::validate(schema.as_json(), &candidate);
    if validation.valid {
        return None;
    }
    validation
        .errors
        .first()
        .map(std::string::ToString::to_string)
}

fn prompt_name(prompt: &Prompt) -> Option<String> {
    serde_json::to_value(prompt).ok().and_then(|value| {
        value
            .get("name")
            .and_then(|name| name.as_str())
            .map(str::to_string)
    })
}

fn resource_uri(resource: &Resource) -> Option<String> {
    serde_json::to_value(resource).ok().and_then(|value| {
        value
            .get("uri")
            .and_then(|uri| uri.as_str())
            .map(str::to_string)
    })
}

fn resource_template_uri(resource_template: &ResourceTemplate) -> Option<String> {
    serde_json::to_value(resource_template)
        .ok()
        .and_then(|value| {
            value
                .get("uriTemplate")
                .and_then(|uri| uri.as_str())
                .map(str::to_string)
        })
}

fn invalid_params_error(message: String) -> rmcp::ErrorData {
    rmcp::ErrorData::new(rmcp::model::ErrorCode::INVALID_PARAMS, message, None)
}

fn resource_template_matches(template: &str, uri: &str) -> bool {
    let literals = template_literals(template);
    if literals.is_empty() {
        return template == uri;
    }
    let Some(first) = literals.first() else {
        return false;
    };
    if !uri.starts_with(first) {
        return false;
    }
    let mut index = first.len();
    for literal in literals.iter().skip(1) {
        if literal.is_empty() {
            continue;
        }
        let Some(found) = uri[index..].find(literal) else {
            return false;
        };
        index += found + literal.len();
    }
    if !template.ends_with('}')
        && let Some(last) = literals.last()
        && !last.is_empty()
    {
        return uri.ends_with(last);
    }
    true
}

#[async_trait]
impl Component for RegistryHandler {
    fn name(&self) -> &str {
        &self.name
    }

    async fn start(&self) -> rskit_errors::AppResult<()> {
        Ok(())
    }

    async fn stop(&self) -> rskit_errors::AppResult<()> {
        Ok(())
    }

    fn health(&self) -> Health {
        Health::healthy(self.name())
    }
}

fn template_literals(template: &str) -> Vec<String> {
    let mut literals = Vec::new();
    let mut current = String::new();
    let mut depth = 0;
    for ch in template.chars() {
        match ch {
            '{' if depth == 0 => {
                literals.push(std::mem::take(&mut current));
                depth += 1;
            }
            '{' => depth += 1,
            '}' if depth > 0 => depth -= 1,
            _ if depth == 0 => current.push(ch),
            _ => {}
        }
    }
    literals.push(current);
    literals
}

#[cfg(test)]
mod tests {
    use super::*;
    use parking_lot::Mutex;

    use rskit_schema::ValidationResult;
    use rskit_tool::{Callable, Definition, ToolResult, from_fn, text_result};
    use schemars::JsonSchema;
    use serde::Deserialize;
    use serde_json::json;

    #[derive(Deserialize, JsonSchema)]
    struct EchoInput {
        message: String,
    }

    fn test_registry() -> Arc<Registry> {
        let registry = Registry::new();
        registry
            .register(
                from_fn(
                    "echo",
                    "Echo a message back",
                    |_ctx: Context, input: EchoInput| async move {
                        Ok(text_result(&input.message))
                    },
                )
                .unwrap(),
            )
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
            ..Default::default()
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
            ..Default::default()
        };
        let handler = create_server("test", "0.1.0", test_registry(), config);
        let tools = handler.mcp_tools();
        assert_eq!(tools[0].name.as_ref(), "pre_echo");
    }

    #[test]
    fn test_allowed_tools_filter_list_and_lookup() {
        let config = ServerConfig {
            allowed_tools: vec!["echo".to_string()],
            ..Default::default()
        };
        let handler = create_server("test", "0.1.0", test_registry(), config);

        let tools = handler.mcp_tools();
        assert_eq!(tools.len(), 1);
        assert_eq!(tools[0].name.as_ref(), "echo");
        assert!(handler.get_tool("echo").is_some());
        assert!(handler.get_tool("missing").is_none());
    }

    struct DenyAuthorizer;

    #[async_trait]
    impl ToolAuthorizer for DenyAuthorizer {
        async fn authorize_tool(
            &self,
            request: &ToolAuthorizationRequest,
        ) -> Result<ToolAuthorizationDecision, String> {
            if request.tool_name == "echo" {
                return Ok(ToolAuthorizationDecision {
                    allowed: false,
                    reason: String::from("echo disabled"),
                });
            }
            Ok(ToolAuthorizationDecision {
                allowed: true,
                reason: String::from("allowed"),
            })
        }
    }

    struct RecordingAuditSink {
        events: Arc<Mutex<Vec<ToolAuditEvent>>>,
    }

    #[async_trait]
    impl ToolAuditSink for RecordingAuditSink {
        async fn record_tool_call(&self, event: ToolAuditEvent) {
            self.events.lock().push(event);
        }
    }

    #[tokio::test]
    async fn test_tool_authorizer_and_audit_sink() {
        let events = Arc::new(Mutex::new(Vec::new()));
        let config = ServerConfig {
            tool_authorizer: Some(Arc::new(DenyAuthorizer)),
            tool_audit_sink: Some(Arc::new(RecordingAuditSink {
                events: Arc::clone(&events),
            })),
            ..Default::default()
        };
        let handler = create_server("test", "0.1.0", test_registry(), config);

        let request: CallToolRequestParams = serde_json::from_value(json!({
            "name": "echo",
            "arguments": {
                "message": "hi"
            }
        }))
        .unwrap();
        let result = handler.handle_call_tool(request).await;

        assert_eq!(result.is_error, Some(true));
        assert_eq!(first_text(&result), Some("tool call denied: echo disabled"));

        let captured = events.lock();
        assert_eq!(captured.len(), 1);
        assert_eq!(captured[0].tool_name, "echo");
        assert_eq!(captured[0].outcome, "denied");
    }

    #[tokio::test]
    async fn test_max_input_bytes() {
        let config = ServerConfig {
            max_input_bytes: 8,
            ..Default::default()
        };
        let handler = create_server("test", "0.1.0", test_registry(), config);

        let request: CallToolRequestParams = serde_json::from_value(json!({
            "name": "echo",
            "arguments": {
                "message": "hello"
            }
        }))
        .unwrap();
        let result = handler.handle_call_tool(request).await;

        assert_eq!(result.is_error, Some(true));
        assert_eq!(
            first_text(&result),
            Some("input too large: exceeds 8 bytes")
        );
    }

    struct InvalidOutputTool {
        definition: Definition,
    }

    #[async_trait]
    impl Callable for InvalidOutputTool {
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
                output: Some(json!({"sum": "bad"}).into()),
                content: String::from("{\"sum\":\"bad\"}"),
                is_error: false,
                metadata: rskit_tool::ToolMetadata::new(),
            })
        }
    }

    #[tokio::test]
    async fn test_output_schema_validation() {
        let registry = Registry::new();
        registry
            .register(Box::new(InvalidOutputTool {
                definition: Definition {
                    name: String::from("bad_output"),
                    description: String::from("Return invalid output"),
                    input_schema: rskit_tool::ToolSchema::new(
                        json!({"type": "object", "properties": {}}),
                    )
                    .unwrap(),
                    output_schema: Some(
                        rskit_tool::ToolSchema::new(json!({
                            "type": "object",
                            "properties": {"sum": {"type": "integer"}},
                            "required": ["sum"]
                        }))
                        .unwrap(),
                    ),
                    annotations: rskit_tool::Annotations::default(),
                    envelope: rskit_tool::Envelope::default(),
                },
            }))
            .unwrap();
        let handler = create_server("test", "0.1.0", Arc::new(registry), Default::default());

        let request: CallToolRequestParams = serde_json::from_value(json!({
            "name": "bad_output",
            "arguments": {}
        }))
        .unwrap();
        let result = handler.handle_call_tool(request).await;

        assert_eq!(result.is_error, Some(true));
        assert!(
            first_text(&result)
                .unwrap_or_default()
                .starts_with("output validation error:")
        );
    }

    #[tokio::test]
    async fn test_prompts_resources_and_templates() {
        let prompt: Prompt = serde_json::from_value(json!({
            "name": "greet",
            "description": "Render a greeting prompt",
            "arguments": [{"name": "name", "required": true}]
        }))
        .unwrap();
        let resource: Resource = serde_json::from_value(json!({
            "uri": "memo://info",
            "name": "info",
            "mimeType": "text/plain"
        }))
        .unwrap();
        let template: ResourceTemplate = serde_json::from_value(json!({
            "uriTemplate": "memo://items/{id}",
            "name": "item",
            "mimeType": "text/plain"
        }))
        .unwrap();

        let config = ServerConfig {
            prompts: vec![PromptEntry::new(prompt, |request| async move {
                let name = request
                    .arguments
                    .as_ref()
                    .and_then(|arguments| arguments.get("name"))
                    .and_then(serde_json::Value::as_str)
                    .unwrap_or_default()
                    .to_owned();
                serde_json::from_value(json!({
                    "description": "Greeting prompt",
                    "messages": [{
                        "role": "user",
                        "content": {"type": "text", "text": format!("Say hello to {name}")}
                    }]
                }))
                .map_err(|err| invalid_params_error(err.to_string()))
            })],
            resources: vec![ResourceEntry::new(resource, |request| async move {
                serde_json::from_value(json!({
                    "contents": [{
                        "uri": request.uri.to_string(),
                        "mimeType": "text/plain",
                        "text": "info"
                    }]
                }))
                .map_err(|err| invalid_params_error(err.to_string()))
            })],
            resource_templates: vec![ResourceTemplateEntry::new(template, |request| async move {
                serde_json::from_value(json!({
                    "contents": [{
                        "uri": request.uri.to_string(),
                        "mimeType": "text/plain",
                        "text": format!("templated:{}", request.uri)
                    }]
                }))
                .map_err(|err| invalid_params_error(err.to_string()))
            })],
            ..Default::default()
        };
        let handler = create_server("test", "0.1.0", test_registry(), config);

        let prompts = handler.mcp_prompts();
        assert_eq!(prompt_name(&prompts[0]).as_deref(), Some("greet"));

        let prompt_result = handler
            .handle_get_prompt(
                serde_json::from_value(json!({
                    "name": "greet",
                    "arguments": {"name": "World"}
                }))
                .unwrap(),
            )
            .await
            .unwrap();
        let prompt_json = serde_json::to_value(&prompt_result).unwrap();
        assert_eq!(
            prompt_json["messages"][0]["content"]["text"].as_str(),
            Some("Say hello to World")
        );

        let resources = handler.mcp_resources();
        assert_eq!(resource_uri(&resources[0]).as_deref(), Some("memo://info"));

        let templates = handler.mcp_resource_templates();
        assert_eq!(
            resource_template_uri(&templates[0]).as_deref(),
            Some("memo://items/{id}")
        );

        let resource_result = handler
            .handle_read_resource(serde_json::from_value(json!({"uri": "memo://info"})).unwrap())
            .await
            .unwrap();
        let resource_json = serde_json::to_value(&resource_result).unwrap();
        assert_eq!(resource_json["contents"][0]["text"].as_str(), Some("info"));

        let templated_result = handler
            .handle_read_resource(
                serde_json::from_value(json!({"uri": "memo://items/123"})).unwrap(),
            )
            .await
            .unwrap();
        let templated_json = serde_json::to_value(&templated_result).unwrap();
        assert_eq!(
            templated_json["contents"][0]["text"].as_str(),
            Some("templated:memo://items/123")
        );
    }

    #[tokio::test]
    async fn test_prompt_and_resource_not_found_errors() {
        let handler = create_server("test", "0.1.0", test_registry(), Default::default());

        let prompt_error = handler
            .handle_get_prompt(serde_json::from_value(json!({"name": "missing"})).unwrap())
            .await
            .expect_err("missing prompt is rejected");
        assert!(prompt_error.message.contains("prompt not found"));

        let resource_error = handler
            .handle_read_resource(serde_json::from_value(json!({"uri": "memo://missing"})).unwrap())
            .await
            .expect_err("missing resource is rejected");
        assert!(resource_error.message.contains("resource not found"));
    }

    #[test]
    fn test_resource_template_matching_edges() {
        assert!(resource_template_matches(
            "memo://items/{id}",
            "memo://items/123"
        ));
        assert!(resource_template_matches(
            "memo://{tenant}/items/{id}/details",
            "memo://acme/items/123/details"
        ));
        assert!(!resource_template_matches(
            "memo://items/{id}",
            "file://items/123"
        ));
        assert!(!resource_template_matches(
            "memo://items/{id}/details",
            "memo://items/123/summary"
        ));
        assert!(resource_template_matches(
            "memo://literal",
            "memo://literal"
        ));
        assert!(!resource_template_matches("memo://literal", "memo://other"));
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
}
