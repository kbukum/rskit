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

#[cfg(test)]
mod tests {
    use rskit_schema::{ValidationError, ValidationResult};
    use rskit_tool::result::{error_result, json_result, text_result};

    use super::*;

    #[test]
    fn validation_details_use_default_message_when_schema_has_no_errors() {
        assert_eq!(
            validation_error_details(&ValidationResult {
                valid: false,
                errors: Vec::new(),
            }),
            "schema validation failed"
        );
        assert_eq!(
            validation_error_details(&ValidationResult {
                valid: false,
                errors: vec![ValidationError {
                    path: "field".to_string(),
                    message: "required".to_string(),
                }],
            }),
            "field: required"
        );
    }

    #[test]
    fn result_size_prefers_structured_output_and_skips_error_output_validation() {
        let json = json_result(&serde_json::json!({"ok": true})).unwrap();
        assert_eq!(
            result_size_bytes(&json),
            json_size_bytes(json.output.as_ref().unwrap().as_json())
        );
        assert_eq!(result_size_bytes(&text_result("hello")), 5);

        let definition = rskit_tool::Definition {
            name: "demo".to_string(),
            description: "demo".to_string(),
            input_schema: rskit_tool::ToolSchema::any_object(),
            output_schema: Some(
                rskit_tool::ToolSchema::new(serde_json::json!({
                    "type": "object",
                    "required": ["ok"],
                    "properties": {"ok": {"type": "boolean"}}
                }))
                .unwrap(),
            ),
            annotations: rskit_tool::Annotations::default(),
            envelope: rskit_tool::Envelope::default(),
        };

        assert!(validate_tool_output(&definition, &json).is_none());
        assert!(validate_tool_output(&definition, &error_result("bad")).is_none());
        assert!(validate_tool_output(&definition, &text_result("not an object")).is_some());
    }
}
