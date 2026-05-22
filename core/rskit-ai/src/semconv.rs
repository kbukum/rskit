//! OpenTelemetry GenAI semantic convention attribute keys.

/// GenAI system identifier, for example `openai`, `anthropic`, or `triton`.
pub const SYSTEM: &str = "gen_ai.system";
/// GenAI operation name.
pub const OPERATION_NAME: &str = "gen_ai.operation.name";
/// Requested model name.
pub const REQUEST_MODEL: &str = "gen_ai.request.model";
/// Requested model version.
pub const REQUEST_MODEL_VERSION: &str = "gen_ai.request.model.version";
/// Input token usage.
pub const USAGE_INPUT_TOKENS: &str = "gen_ai.usage.input_tokens";
/// Output token usage.
pub const USAGE_OUTPUT_TOKENS: &str = "gen_ai.usage.output_tokens";
/// Cached token usage.
pub const USAGE_CACHED_TOKENS: &str = "gen_ai.usage.cached_tokens";
/// Reasoning token usage.
pub const USAGE_REASONING_TOKENS: &str = "gen_ai.usage.reasoning_tokens";
/// Response finish reason.
pub const RESPONSE_FINISH_REASON: &str = "gen_ai.response.finish_reason";
/// Request maximum output token count.
pub const REQUEST_MAX_TOKENS: &str = "gen_ai.request.max_tokens";
/// Request temperature.
pub const REQUEST_TEMPERATURE: &str = "gen_ai.request.temperature";
/// Response model name.
pub const RESPONSE_MODEL: &str = "gen_ai.response.model";
/// Tool name associated with a GenAI operation.
pub const TOOL_NAME: &str = "gen_ai.tool.name";
/// Provider request identifier.
pub const REQUEST_ID: &str = "gen_ai.request.id";

/// Canonical values for [`OPERATION_NAME`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Operation {
    /// Chat completion operation.
    Chat,
    /// Text completion operation.
    TextCompletion,
    /// Embedding generation operation.
    Embedding,
    /// Agent run operation.
    AgentRun,
    /// Agent turn operation.
    AgentTurn,
    /// LLM provider call operation.
    LlmCall,
    /// Tool invocation operation.
    ToolCall,
    /// MCP request operation.
    McpRequest,
    /// Streaming operation.
    Stream,
    /// Model-serving inference request operation.
    InferenceRequest,
}

impl Operation {
    /// Return the canonical `gen_ai.operation.name` value.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Chat => "chat",
            Self::TextCompletion => "text_completion",
            Self::Embedding => "embeddings",
            Self::AgentRun => "agent.run",
            Self::AgentTurn => "agent.turn",
            Self::LlmCall => "llm.call",
            Self::ToolCall => "tool.call",
            Self::McpRequest => "mcp.request",
            Self::Stream => "stream",
            Self::InferenceRequest => "inference.request",
        }
    }

    /// Parse a canonical `gen_ai.operation.name` value.
    #[must_use]
    pub fn from_operation_name(value: &str) -> Option<Self> {
        match value {
            "chat" => Some(Self::Chat),
            "text_completion" => Some(Self::TextCompletion),
            "embeddings" | "embedding" => Some(Self::Embedding),
            "agent.run" => Some(Self::AgentRun),
            "agent.turn" => Some(Self::AgentTurn),
            "llm.call" => Some(Self::LlmCall),
            "tool.call" => Some(Self::ToolCall),
            "mcp.request" => Some(Self::McpRequest),
            "stream" => Some(Self::Stream),
            "inference.request" => Some(Self::InferenceRequest),
            _ => None,
        }
    }
}
