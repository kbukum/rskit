#![cfg_attr(not(test), allow(dead_code))]

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct StreamToolCall {
    pub(crate) index: usize,
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) input_delta: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct StreamChunk {
    pub(crate) content: String,
    pub(crate) tool_calls: Vec<StreamToolCall>,
    pub(crate) done: bool,
}
