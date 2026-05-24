#[derive(Debug, Clone, Default, PartialEq, Eq)]
/// Incremental tool-call state extracted from a provider stream event.
pub struct StreamToolCall {
    /// Tool-call position in the provider response.
    pub index: usize,
    /// Provider-supplied tool-call identifier.
    pub id: String,
    /// Tool/function name.
    pub name: String,
    /// Incremental JSON arguments payload.
    pub input_delta: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
/// Normalized content/tool delta extracted from a provider stream event.
pub struct StreamChunk {
    /// Text content emitted by the event.
    pub content: String,
    /// Tool-call deltas emitted by the event.
    pub tool_calls: Vec<StreamToolCall>,
    /// Whether the provider indicates the stream is finished.
    pub done: bool,
}
