//! Conversions between rskit tool types and MCP protocol types.

use std::sync::Arc;

use rmcp::model::{
    CallToolResult, Content, ErrorData, ListToolsResult, RawContent, Tool, ToolAnnotations,
};
use rskit_tool::result::ToolResult;
use rskit_tool::{Annotations, Definition};

// ── Kit Definition → MCP Tool ──────────────────────────────────────────────

/// Convert an rskit [`Definition`] to an MCP [`Tool`].
///
/// An optional `prefix` is prepended to the tool name (e.g. `"myserver_"`).
pub fn definition_to_tool(def: &Definition, prefix: &str) -> Tool {
    let name = if prefix.is_empty() {
        def.name.clone()
    } else {
        format!("{prefix}{}", def.name)
    };

    let input_schema = value_to_json_object(&def.input_schema);

    let mut tool = Tool::new(name, def.description.clone(), input_schema);

    if let Some(ref ann) = def.annotations {
        let mut mcp_ann = ToolAnnotations::new();
        if let Some(ref title) = ann.title {
            mcp_ann = ToolAnnotations::with_title(title.clone());
        }
        if let Some(ro) = ann.read_only_hint {
            mcp_ann = mcp_ann.read_only(ro);
        }
        if let Some(dest) = ann.destructive_hint {
            mcp_ann = mcp_ann.destructive(dest);
        }
        if let Some(idem) = ann.idempotent_hint {
            mcp_ann = mcp_ann.idempotent(idem);
        }
        if let Some(ow) = ann.open_world_hint {
            mcp_ann = mcp_ann.open_world(ow);
        }
        tool = tool.with_annotations(mcp_ann);
    }

    if let Some(ref output_schema) = def.output_schema {
        if let Some(obj) = output_schema.as_object() {
            tool = tool.with_raw_output_schema(Arc::new(obj.clone()));
        }
    }

    tool
}

/// Convert a list of rskit [`Definition`]s to an MCP [`ListToolsResult`].
pub fn definitions_to_list_result(defs: &[Definition], prefix: &str) -> ListToolsResult {
    let tools: Vec<Tool> = defs.iter().map(|d| definition_to_tool(d, prefix)).collect();
    ListToolsResult {
        tools,
        next_cursor: None,
        meta: None,
    }
}

// ── MCP Tool → Kit Definition ──────────────────────────────────────────────

/// Convert an MCP [`Tool`] to an rskit [`Definition`].
///
/// An optional `prefix` is stripped from the tool name.
pub fn tool_to_definition(tool: &Tool, prefix: &str) -> Definition {
    let raw_name = tool.name.as_ref();
    let name = if !prefix.is_empty() && raw_name.starts_with(prefix) {
        raw_name[prefix.len()..].to_string()
    } else {
        raw_name.to_string()
    };

    let input_schema = tool.schema_as_json_value();

    let output_schema = tool
        .output_schema
        .as_ref()
        .map(|s| serde_json::to_value(s.as_ref()).unwrap_or_default());

    let annotations = tool.annotations.as_ref().map(|a| Annotations {
        title: a.title.clone(),
        read_only_hint: a.read_only_hint,
        destructive_hint: a.destructive_hint,
        idempotent_hint: a.idempotent_hint,
        open_world_hint: a.open_world_hint,
        execution_hint: None,
        category: None,
        tags: None,
    });

    let read_only = tool
        .annotations
        .as_ref()
        .and_then(|a| a.read_only_hint)
        .unwrap_or(false);
    let destructive = tool
        .annotations
        .as_ref()
        .and_then(|a| a.destructive_hint)
        .unwrap_or(false);

    Definition {
        name,
        description: tool.description.as_deref().unwrap_or("").to_string(),
        input_schema,
        output_schema,
        annotations,
        read_only,
        destructive,
        max_result_size: 0,
        timeout_secs: 0.0,
    }
}

// ── Kit ToolResult → MCP CallToolResult ────────────────────────────────────

/// Convert an rskit [`ToolResult`] to an MCP [`CallToolResult`].
pub fn tool_result_to_call_result(result: &ToolResult) -> CallToolResult {
    let content = vec![Content::text(&result.content)];

    if result.is_error {
        let mut r = CallToolResult::error(content);
        if let Some(ref output) = result.output {
            r.structured_content = Some(output.clone());
        }
        r
    } else {
        match result.output {
            Some(ref output) => {
                let mut r = CallToolResult::structured(output.clone());
                r.content = content;
                r
            }
            None => CallToolResult::success(content),
        }
    }
}

/// Convert an rskit [`AppError`](rskit_errors::AppError) to an MCP [`ErrorData`].
pub fn app_error_to_mcp_error(err: &rskit_errors::AppError) -> ErrorData {
    ErrorData::new(
        rmcp::model::ErrorCode::INTERNAL_ERROR,
        err.message.clone(),
        None,
    )
}

// ── MCP CallToolResult → Kit ToolResult ────────────────────────────────────

/// Convert an MCP [`CallToolResult`] to an rskit [`ToolResult`].
pub fn call_result_to_tool_result(result: &CallToolResult) -> ToolResult {
    let content: String = result
        .content
        .iter()
        .filter_map(|c| {
            if let RawContent::Text(text) = &c.raw {
                Some(text.text.as_str())
            } else {
                None
            }
        })
        .collect::<Vec<_>>()
        .join("\n");

    let output = result.structured_content.clone();
    let is_error = result.is_error.unwrap_or(false);

    ToolResult {
        output,
        content,
        is_error,
        metadata: std::collections::HashMap::new(),
    }
}

// ── Helpers ────────────────────────────────────────────────────────────────

/// Convert a `serde_json::Value` to an MCP `JsonObject` (`Map<String, Value>`).
fn value_to_json_object(value: &serde_json::Value) -> serde_json::Map<String, serde_json::Value> {
    match value {
        serde_json::Value::Object(map) => map.clone(),
        _ => {
            let mut map = serde_json::Map::new();
            map.insert(
                "type".to_string(),
                serde_json::Value::String("object".to_string()),
            );
            map
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rskit_tool::result::{ToolResult, error_result, text_result};
    use rskit_tool::{Annotations, Definition};
    use serde_json::json;

    fn sample_definition() -> Definition {
        Definition {
            name: "search".to_string(),
            description: "Search the web".to_string(),
            input_schema: json!({
                "type": "object",
                "properties": {
                    "query": {"type": "string"}
                },
                "required": ["query"]
            }),
            output_schema: None,
            annotations: Some(Annotations {
                title: Some("Web Search".to_string()),
                read_only_hint: Some(true),
                destructive_hint: Some(false),
                idempotent_hint: Some(true),
                open_world_hint: Some(true),
                category: Some("web".to_string()),
                tags: Some(vec!["search".to_string()]),
            }),
            read_only: true,
            destructive: false,
            max_result_size: 0,
            timeout_secs: 0.0,
        }
    }

    #[test]
    fn test_definition_to_tool_no_prefix() {
        let def = sample_definition();
        let tool = definition_to_tool(&def, "");
        assert_eq!(tool.name.as_ref(), "search");
        assert_eq!(tool.description.as_deref(), Some("Search the web"));
        let ann = tool.annotations.as_ref().unwrap();
        assert_eq!(ann.title.as_deref(), Some("Web Search"));
        assert_eq!(ann.read_only_hint, Some(true));
    }

    #[test]
    fn test_definition_to_tool_with_prefix() {
        let def = sample_definition();
        let tool = definition_to_tool(&def, "myserver_");
        assert_eq!(tool.name.as_ref(), "myserver_search");
    }

    #[test]
    fn test_tool_to_definition_strips_prefix() {
        let def = sample_definition();
        let tool = definition_to_tool(&def, "myserver_");
        let round_tripped = tool_to_definition(&tool, "myserver_");
        assert_eq!(round_tripped.name, "search");
        assert_eq!(round_tripped.description, "Search the web");
    }

    #[test]
    fn test_tool_to_definition_no_prefix() {
        let def = sample_definition();
        let tool = definition_to_tool(&def, "");
        let round_tripped = tool_to_definition(&tool, "");
        assert_eq!(round_tripped.name, "search");
    }

    #[test]
    fn test_tool_result_to_call_result_success() {
        let result = text_result("hello world");
        let mcp_result = tool_result_to_call_result(&result);
        assert_eq!(mcp_result.content.len(), 1);
        assert_eq!(mcp_result.is_error, Some(false));
    }

    #[test]
    fn test_tool_result_to_call_result_error() {
        let result = error_result("something failed");
        let mcp_result = tool_result_to_call_result(&result);
        assert_eq!(mcp_result.is_error, Some(true));
    }

    #[test]
    fn test_tool_result_with_structured_output() {
        let result = ToolResult {
            output: Some(json!({"count": 42})),
            content: "42 results".to_string(),
            is_error: false,
            metadata: std::collections::HashMap::new(),
        };
        let mcp_result = tool_result_to_call_result(&result);
        assert_eq!(mcp_result.structured_content, Some(json!({"count": 42})));
    }

    #[test]
    fn test_call_result_to_tool_result() {
        let mcp_result = CallToolResult::success(vec![Content::text("result text")]);
        let tool_result = call_result_to_tool_result(&mcp_result);
        assert_eq!(tool_result.content, "result text");
        assert!(!tool_result.is_error);
    }

    #[test]
    fn test_call_result_error_to_tool_result() {
        let mcp_result = CallToolResult::error(vec![Content::text("error msg")]);
        let tool_result = call_result_to_tool_result(&mcp_result);
        assert_eq!(tool_result.content, "error msg");
        assert!(tool_result.is_error);
    }

    #[test]
    fn test_definitions_to_list_result() {
        let defs = vec![sample_definition()];
        let result = definitions_to_list_result(&defs, "");
        assert_eq!(result.tools.len(), 1);
        assert_eq!(result.tools[0].name.as_ref(), "search");
    }

    #[test]
    fn test_roundtrip_annotations_preserved() {
        let def = sample_definition();
        let tool = definition_to_tool(&def, "");
        let round_tripped = tool_to_definition(&tool, "");
        let ann = round_tripped.annotations.unwrap();
        assert_eq!(ann.title, Some("Web Search".to_string()));
        assert_eq!(ann.read_only_hint, Some(true));
        assert_eq!(ann.destructive_hint, Some(false));
        assert_eq!(ann.idempotent_hint, Some(true));
        assert_eq!(ann.open_world_hint, Some(true));
    }

    #[test]
    fn test_value_to_json_object_non_object() {
        let obj = value_to_json_object(&json!(42));
        assert_eq!(obj.get("type").and_then(|v| v.as_str()), Some("object"));
    }
}
