#![cfg_attr(not(test), allow(dead_code))]

use rskit_ai::ToolUseBlock;
use rskit_errors::{AppError, AppResult, ErrorCode};
use serde_json::{Map, Value};

use crate::{StreamChunk, StreamToolCall};

/// Merge a streamed tool-call delta into the accumulated call list.
pub fn merge_tool_delta(calls: &mut Vec<StreamToolCall>, delta: StreamToolCall) {
    if calls.len() <= delta.index {
        calls.resize_with(delta.index + 1, StreamToolCall::default);
    }

    let current = &mut calls[delta.index];
    current.index = delta.index;

    if !delta.id.is_empty() {
        current.id = delta.id;
    }
    if !delta.name.is_empty() {
        current.name = delta.name;
    }
    current.input_delta.push_str(&delta.input_delta);
}

/// Parse a provider tool-input JSON fragment into a JSON object map.
pub fn parse_input_json(input: &str) -> AppResult<Map<String, Value>> {
    if input.trim().is_empty() {
        return Ok(Map::new());
    }

    let value: Value = serde_json::from_str(input).map_err(|error| {
        AppError::new(
            ErrorCode::InvalidFormat,
            format!("failed to parse tool input JSON: {error}"),
        )
    })?;

    value_to_input_map(value)
}

/// Convert a JSON value into a tool-input object map.
pub fn value_to_input_map(value: Value) -> AppResult<Map<String, Value>> {
    match value {
        Value::Object(map) => Ok(map),
        Value::Null => Ok(Map::new()),
        other => Err(AppError::new(
            ErrorCode::InvalidFormat,
            format!("expected tool input object, got {other}"),
        )),
    }
}

/// Reconstruct tool-use blocks from a sequence of streamed chunks.
pub fn accumulate_tool_uses(
    chunks: impl IntoIterator<Item = StreamChunk>,
) -> AppResult<Vec<ToolUseBlock>> {
    let mut calls = Vec::new();
    for chunk in chunks {
        for delta in chunk.tool_calls {
            merge_tool_delta(&mut calls, delta);
        }
    }

    calls
        .into_iter()
        .enumerate()
        .filter(|(_, call)| !call.name.is_empty() || !call.id.is_empty())
        .map(|(index, call)| {
            Ok(ToolUseBlock {
                id: if call.id.is_empty() {
                    format!("tool_call_{index}")
                } else {
                    call.id
                },
                name: call.name,
                input: parse_input_json(&call.input_delta)?,
            })
        })
        .collect()
}
