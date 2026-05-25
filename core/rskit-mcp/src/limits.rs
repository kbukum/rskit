//! MCP request/result size and output validation helpers.

pub(crate) fn json_size_bytes(value: &serde_json::Value) -> usize {
    serde_json::to_vec(value).map_or(0, |bytes| bytes.len())
}

pub(crate) fn validation_error_details(validation: &rskit_schema::ValidationResult) -> String {
    let details = validation
        .errors
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("; ");
    if details.is_empty() {
        String::from("schema validation failed")
    } else {
        details
    }
}

pub(crate) fn result_size_bytes(result: &rskit_tool::result::ToolResult) -> usize {
    if let Some(output) = &result.output {
        return json_size_bytes(output.as_json());
    }
    result.content.len()
}

pub(crate) fn validate_tool_output(
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
