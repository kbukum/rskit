//! Behavioral tests for chat messages, content blocks, requests, and tool contracts.

use rskit_llm::{
    AssistantMessage, CompletionRequest, CompletionResponse, ContentPart, FinishReason, Message,
    ToolChoice, ToolDefinition, ToolUseBlock, Usage, assistant, system, text_of, tool_result_msg,
    user,
};

const _: fn() = || {
    fn assert_copy<T: Copy>() {}
    assert_copy::<Usage>();
};

// ═══════════════════════════════════════════════════════════════════════════
// Message enum — role helpers
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn message_user_role() {
    assert_eq!(user("hi").role(), "user");
}

#[test]
fn message_assistant_role() {
    assert_eq!(assistant("hi").role(), "assistant");
}

#[test]
fn message_system_role() {
    assert_eq!(system("hi").role(), "system");
}

#[test]
fn message_tool_result_role() {
    assert_eq!(tool_result_msg("id", "ok", false).role(), "tool_result");
}

#[test]
fn message_serde_roundtrip_preserves_content() {
    let msg = user("round trip content");
    let json = serde_json::to_string(&msg).unwrap();
    let back: Message = serde_json::from_str(&json).unwrap();
    assert_eq!(
        serde_json::to_value(&back).unwrap(),
        serde_json::to_value(&msg).unwrap()
    );
}

#[test]
fn message_debug_format() {
    let msg = system("debug");
    let dbg = format!("{:?}", msg);
    assert!(
        dbg.contains("System") || dbg.contains("system"),
        "Debug format = {dbg}"
    );
}

#[test]
fn message_serde_roundtrip_user() {
    let msg = user("round trip");
    let json = serde_json::to_string(&msg).unwrap();
    let back: Message = serde_json::from_str(&json).unwrap();
    assert_eq!(back.role(), "user");
}

#[test]
fn message_serde_roundtrip_assistant() {
    let msg = assistant("hello");
    let json = serde_json::to_string(&msg).unwrap();
    let back: Message = serde_json::from_str(&json).unwrap();
    assert_eq!(back.role(), "assistant");
}

#[test]
fn message_serde_roundtrip_system() {
    let msg = system("sys prompt");
    let json = serde_json::to_string(&msg).unwrap();
    let back: Message = serde_json::from_str(&json).unwrap();
    assert_eq!(back.role(), "system");
}

#[test]
fn message_serde_roundtrip_tool_result() {
    let msg = tool_result_msg("call_1", "output", false);
    let json = serde_json::to_string(&msg).unwrap();
    let back: Message = serde_json::from_str(&json).unwrap();
    assert_eq!(back.role(), "tool_result");
}

#[test]
fn message_deserialize_invalid_role() {
    let json = r#"{"role":"Admin","content":"hi"}"#;
    let result = serde_json::from_str::<Message>(json);
    assert!(result.is_err(), "expected error for invalid role");
}

// ═══════════════════════════════════════════════════════════════════════════
// ContentPart — serde, variants
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn content_block_text_serde() {
    let block = ContentPart::Text { text: "hi".into() };
    let json = serde_json::to_value(&block).unwrap();
    assert_eq!(json["type"], "text");
    assert_eq!(json["text"], "hi");
}

#[test]
fn content_block_image_serde() {
    let block = ContentPart::Image {
        source: "base64data".into(),
        mime_type: "image/png".into(),
        data: None,
    };
    let json = serde_json::to_value(&block).unwrap();
    assert_eq!(json["type"], "image");
    assert_eq!(json["mime_type"], "image/png");
}

#[test]
fn content_block_tool_use_serde() {
    let block = ContentPart::ToolUse {
        id: "call_1".into(),
        name: "search".into(),
        input: serde_json::json!({"q": "rust"})
            .as_object()
            .cloned()
            .unwrap(),
    };
    let json = serde_json::to_value(&block).unwrap();
    assert_eq!(json["type"], "tool_use");
    assert_eq!(json["name"], "search");
    let back: ContentPart = serde_json::from_value(json).unwrap();
    match back {
        ContentPart::ToolUse { name, .. } => assert_eq!(name, "search"),
        _ => panic!("expected ToolUse"),
    }
}

#[test]
fn content_block_tool_result_serde() {
    let block = ContentPart::ToolResult {
        id: "call_1".into(),
        content: "42".into(),
        is_error: false,
    };
    let json = serde_json::to_value(&block).unwrap();
    assert_eq!(json["type"], "tool_result");
}

#[test]
fn content_block_audio_serde() {
    let block = ContentPart::Audio {
        source: "s3://bucket/audio.mp3".into(),
        mime_type: "audio/mpeg".into(),
        data: None,
    };
    let json = serde_json::to_value(&block).unwrap();
    assert_eq!(json["type"], "audio");
}

// ═══════════════════════════════════════════════════════════════════════════
// text_of helper
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn text_of_concatenates_text_blocks() {
    let blocks = vec![
        ContentPart::Text {
            text: "Hello ".into(),
        },
        ContentPart::ToolUse {
            id: "x".into(),
            name: "y".into(),
            input: serde_json::json!({}).as_object().cloned().unwrap(),
        },
        ContentPart::Text {
            text: "world".into(),
        },
    ];
    assert_eq!(text_of(&blocks), "Hello world");
}

#[test]
fn text_of_empty_blocks() {
    assert_eq!(text_of(&[]), "");
}

#[test]
fn text_of_no_text_blocks() {
    let blocks = vec![ContentPart::File {
        source: "s3://bucket/doc.pdf".into(),
        mime_type: "application/pdf".into(),
        data: None,
    }];
    assert_eq!(text_of(&blocks), "");
}

// ═══════════════════════════════════════════════════════════════════════════
// CompletionRequest — construction, serialization, edge cases
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn completion_request_minimal() {
    let req = CompletionRequest {
        model: String::new(),
        messages: vec![],
        max_tokens: None,
        temperature: None,
        stream: false,
        tools: None,
        tool_choice: None,
    };
    assert!(req.model.is_empty());
    assert!(req.messages.is_empty());
    assert!(req.max_tokens.is_none());
    assert!(req.temperature.is_none());
    assert!(!req.stream);
}

#[test]
fn completion_request_all_fields() {
    let tool_def = ToolDefinition {
        name: "get_weather".into(),
        description: "Get weather for a city".into(),
        input_schema: rskit_tool::ToolSchema::new(serde_json::json!({
            "type": "object",
            "properties": {"city": {"type": "string"}}
        }))
        .unwrap(),
        output_schema: None,
    };
    let req = CompletionRequest {
        model: "gpt-4o".into(),
        messages: vec![
            system("Be concise."),
            user("Explain rust."),
            assistant("Rust is a systems language."),
        ],
        max_tokens: Some(4096),
        temperature: Some(0.8),
        stream: true,
        tools: Some(vec![tool_def]),
        tool_choice: Some(ToolChoice::auto()),
    };
    assert_eq!(req.model, "gpt-4o");
    assert_eq!(req.messages.len(), 3);
    assert_eq!(req.max_tokens, Some(4096));
    assert_eq!(req.temperature, Some(0.8));
    assert!(req.stream);
    assert_eq!(req.tools.as_ref().unwrap().len(), 1);
    assert_eq!(req.tools.as_ref().unwrap()[0].name, "get_weather");
    assert_eq!(req.tool_choice.as_ref().unwrap().mode, "auto");
}

#[test]
fn completion_request_zero_temperature() {
    let req = CompletionRequest {
        model: "m".into(),
        messages: vec![],
        max_tokens: None,
        temperature: Some(0.0),
        stream: false,
        tools: None,
        tool_choice: None,
    };
    assert_eq!(req.temperature, Some(0.0));
}

#[test]
fn completion_request_serialize() {
    let req = CompletionRequest {
        model: "gpt-4".into(),
        messages: vec![user("hi")],
        max_tokens: Some(100),
        temperature: Some(0.5),
        stream: false,
        tools: None,
        tool_choice: None,
    };
    let json = serde_json::to_value(&req).unwrap();
    assert_eq!(json["model"], "gpt-4");
    assert_eq!(json["max_tokens"], 100);
    assert_eq!(json["temperature"], 0.5);
    assert_eq!(json["stream"], false);
    assert_eq!(json["messages"][0]["role"], "user");
}

#[test]
fn completion_request_serialize_none_fields() {
    let req = CompletionRequest {
        model: "m".into(),
        messages: vec![],
        max_tokens: None,
        temperature: None,
        stream: false,
        tools: None,
        tool_choice: None,
    };
    let json = serde_json::to_value(&req).unwrap();
    assert!(json["max_tokens"].is_null());
    assert!(json["temperature"].is_null());
}

#[test]
fn completion_request_serde_roundtrip() {
    let req = CompletionRequest {
        model: "claude-3".into(),
        messages: vec![system("sys"), user("usr")],
        max_tokens: Some(500),
        temperature: Some(0.9),
        stream: true,
        tools: None,
        tool_choice: None,
    };
    let json = serde_json::to_string(&req).unwrap();
    let back: CompletionRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(back.model, "claude-3");
    assert_eq!(back.messages.len(), 2);
    assert_eq!(back.max_tokens, Some(500));
    assert_eq!(back.temperature, Some(0.9));
    assert!(back.stream);
}

#[test]
fn completion_request_deserialize_minimal_json() {
    let json = r#"{"model":"m","messages":[],"stream":false}"#;
    let req: CompletionRequest = serde_json::from_str(json).unwrap();
    assert_eq!(req.model, "m");
    assert!(req.messages.is_empty());
    assert!(!req.stream);
}

// ═══════════════════════════════════════════════════════════════════════════
// CompletionResponse — construction, helpers
// ═══════════════════════════════════════════════════════════════════════════

fn make_response(text: &str) -> CompletionResponse {
    CompletionResponse {
        message: AssistantMessage {
            content: vec![ContentPart::Text { text: text.into() }],
            tool_calls: vec![],
            usage: None,
        },
        model: "test".into(),
        usage: Usage {
            input_tokens: 10,
            output_tokens: 5,
            cached_tokens: 0,
            reasoning_tokens: 0,
        },
        stop_reason: Some(FinishReason::Stop),
    }
}

#[test]
fn completion_response_text_helper() {
    let resp = make_response("Hello world!");
    assert_eq!(resp.text(), "Hello world!");
    assert_eq!(resp.model, "test");
    assert_eq!(resp.usage.input_tokens, 10);
    assert_eq!(resp.usage.output_tokens, 5);
}

#[test]
fn completion_response_empty_content() {
    let resp = make_response("");
    assert!(resp.text().is_empty());
}

#[test]
fn completion_response_serde_roundtrip_preserves_text_and_usage() {
    let resp = make_response("test");
    let json = serde_json::to_string(&resp).unwrap();
    let back: CompletionResponse = serde_json::from_str(&json).unwrap();
    assert_eq!(back.text(), "test");
    assert_eq!(
        serde_json::to_value(&back).unwrap(),
        serde_json::to_value(&resp).unwrap()
    );
}

#[test]
fn completion_response_has_tool_calls() {
    let mut resp = make_response("");
    resp.message.tool_calls = vec![ToolUseBlock {
        id: "c1".into(),
        name: "f".into(),
        input: serde_json::Map::new(),
    }];
    resp.stop_reason = Some(FinishReason::ToolUse);
    assert!(resp.has_tool_calls());
}

#[test]
fn completion_response_no_tool_calls() {
    let resp = make_response("done");
    assert!(!resp.has_tool_calls());
}

// ═══════════════════════════════════════════════════════════════════════════
// Usage — deserialization, edge cases
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn usage_deserialize() {
    let json = r#"{"input_tokens": 100, "output_tokens": 50}"#;
    let usage: Usage = serde_json::from_str(json).unwrap();
    assert_eq!(usage.input_tokens, 100);
    assert_eq!(usage.output_tokens, 50);
}

#[test]
fn usage_zero_values() {
    let json = r#"{"input_tokens": 0, "output_tokens": 0}"#;
    let usage: Usage = serde_json::from_str(json).unwrap();
    assert_eq!(usage.input_tokens, 0);
    assert_eq!(usage.output_tokens, 0);
}

#[test]
fn usage_large_values() {
    let json = r#"{"input_tokens": 1000000, "output_tokens": 500000}"#;
    let usage: Usage = serde_json::from_str(json).unwrap();
    assert_eq!(usage.input_tokens, 1_000_000);
    assert_eq!(usage.output_tokens, 500_000);
}

#[test]
fn usage_debug() {
    let usage = Usage {
        input_tokens: 10,
        output_tokens: 5,
        cached_tokens: 0,
        reasoning_tokens: 0,
    };
    let dbg = format!("{:?}", usage);
    assert!(dbg.contains("10"));
    assert!(dbg.contains("5"));
}

// ═══════════════════════════════════════════════════════════════════════════
// FinishReason
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn stop_reason_variants() {
    let reasons = [
        FinishReason::Stop,
        FinishReason::Length,
        FinishReason::ToolUse,
        FinishReason::Stop,
    ];
    for r in &reasons {
        let json = serde_json::to_value(r).unwrap();
        let back: FinishReason = serde_json::from_value(json).unwrap();
        assert_eq!(format!("{:?}", r), format!("{:?}", back));
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// Multi-message conversation construction
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn multi_turn_conversation() {
    let req = CompletionRequest {
        model: "gpt-4".into(),
        messages: vec![
            system("You are a math tutor."),
            user("What is 2+2?"),
            assistant("Let me use a tool."),
            tool_result_msg("call_1", "4", false),
            user("And 3+3?"),
        ],
        max_tokens: Some(50),
        temperature: Some(0.0),
        stream: false,
        tools: None,
        tool_choice: None,
    };
    assert_eq!(req.messages.len(), 5);
    assert_eq!(req.messages[0].role(), "system");
    assert_eq!(req.messages[1].role(), "user");
    assert_eq!(req.messages[2].role(), "assistant");
    assert_eq!(req.messages[3].role(), "tool_result");
    assert_eq!(req.messages[4].role(), "user");
}

// ═══════════════════════════════════════════════════════════════════════════
// JSON format compatibility — verify field names match API expectations
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn completion_request_json_field_names() {
    let req = CompletionRequest {
        model: "m".into(),
        messages: vec![],
        max_tokens: Some(10),
        temperature: Some(0.5),
        stream: true,
        tools: None,
        tool_choice: None,
    };
    let val = serde_json::to_value(&req).unwrap();
    assert!(val.get("model").is_some());
    assert!(val.get("messages").is_some());
    assert!(val.get("max_tokens").is_some());
    assert!(val.get("temperature").is_some());
    assert!(val.get("stream").is_some());
}

#[test]
fn usage_json_field_names() {
    let json = r#"{"input_tokens": 1, "output_tokens": 2}"#;
    let usage: Usage = serde_json::from_str(json).unwrap();
    assert_eq!(usage.input_tokens, 1);
    assert_eq!(usage.output_tokens, 2);
}
