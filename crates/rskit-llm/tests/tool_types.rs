//! Tests for tool-calling types in rskit-llm.

use rskit_llm::{
    CompletionRequest, CompletionResponse, ContentBlock, FunctionCall, Message, ToolCall,
    ToolChoice, Usage, assistant, system, text_of, tool_result_msg, user,
};

// ── ContentBlock ────────────────────────────────────────────────────────────

#[test]
fn content_block_text_serde() {
    let block = ContentBlock::Text {
        text: "hello".into(),
    };
    let json = serde_json::to_value(&block).unwrap();
    assert_eq!(json["type"], "text");
    assert_eq!(json["text"], "hello");
    let back: ContentBlock = serde_json::from_value(json).unwrap();
    match back {
        ContentBlock::Text { text } => assert_eq!(text, "hello"),
        _ => panic!("expected Text"),
    }
}

#[test]
fn content_block_tool_use_serde() {
    let block = ContentBlock::ToolUse {
        id: "call_1".into(),
        name: "search".into(),
        input: serde_json::json!({"q": "rust"}),
    };
    let json = serde_json::to_value(&block).unwrap();
    assert_eq!(json["type"], "tool_use");
    assert_eq!(json["name"], "search");
}

#[test]
fn content_block_tool_result_serde() {
    let block = ContentBlock::ToolResult {
        tool_use_id: "call_1".into(),
        content: "found it".into(),
        is_error: false,
    };
    let json = serde_json::to_value(&block).unwrap();
    assert_eq!(json["type"], "tool_result");
    assert_eq!(json["content"], "found it");
}

// ── Message enum ────────────────────────────────────────────────────────────

#[test]
fn message_user_role() {
    let msg = user("hello");
    assert_eq!(msg.role(), "user");
}

#[test]
fn message_assistant_role() {
    let msg = assistant("hi");
    assert_eq!(msg.role(), "assistant");
}

#[test]
fn message_system_role() {
    let msg = system("be concise");
    assert_eq!(msg.role(), "system");
}

#[test]
fn message_tool_result_role() {
    let msg = tool_result_msg("call_1", "result", false);
    assert_eq!(msg.role(), "tool_result");
}

#[test]
fn message_serde_roundtrip_user() {
    let msg = user("test");
    let json = serde_json::to_value(&msg).unwrap();
    assert_eq!(json["role"], "user");
    let back: Message = serde_json::from_value(json).unwrap();
    assert_eq!(back.role(), "user");
}

#[test]
fn message_serde_roundtrip_system() {
    let msg = system("be helpful");
    let json = serde_json::to_value(&msg).unwrap();
    assert_eq!(json["role"], "system");
    let back: Message = serde_json::from_value(json).unwrap();
    assert_eq!(back.role(), "system");
}

// ── text_of ─────────────────────────────────────────────────────────────────

#[test]
fn text_of_extracts_text_blocks() {
    let blocks = vec![
        ContentBlock::Text {
            text: "Hello ".into(),
        },
        ContentBlock::ToolUse {
            id: "x".into(),
            name: "y".into(),
            input: serde_json::json!({}),
        },
        ContentBlock::Text {
            text: "world".into(),
        },
    ];
    assert_eq!(text_of(&blocks), "Hello world");
}

#[test]
fn text_of_empty() {
    assert_eq!(text_of(&[]), "");
}

// ── ToolCall / FunctionCall ─────────────────────────────────────────────────

#[test]
fn tool_call_serde_roundtrip() {
    let tc = ToolCall {
        id: "call_abc123".into(),
        call_type: "function".into(),
        function: FunctionCall {
            name: "get_weather".into(),
            arguments: r#"{"city":"NYC"}"#.into(),
        },
    };

    let json = serde_json::to_value(&tc).unwrap();
    assert_eq!(json["id"], "call_abc123");
    assert_eq!(json["type"], "function");
    assert_eq!(json["function"]["name"], "get_weather");

    let back: ToolCall = serde_json::from_value(json).unwrap();
    assert_eq!(back.id, "call_abc123");
    assert_eq!(back.function.name, "get_weather");
}

// ── ToolChoice ──────────────────────────────────────────────────────────────

#[test]
fn tool_choice_auto() {
    let tc = ToolChoice::auto();
    assert_eq!(tc.mode, "auto");
    assert!(tc.function.is_none());
}

#[test]
fn tool_choice_none() {
    let tc = ToolChoice::none();
    assert_eq!(tc.mode, "none");
}

#[test]
fn tool_choice_required() {
    let tc = ToolChoice::required();
    assert_eq!(tc.mode, "required");
}

#[test]
fn tool_choice_specific() {
    let tc = ToolChoice::specific("search");
    assert_eq!(tc.mode, "specific");
    assert_eq!(tc.function.as_deref(), Some("search"));
}

#[test]
fn tool_choice_serde_roundtrip() {
    let tc = ToolChoice::specific("search");
    let json = serde_json::to_value(&tc).unwrap();
    let back: ToolChoice = serde_json::from_value(json).unwrap();
    assert_eq!(back.mode, "specific");
    assert_eq!(back.function.as_deref(), Some("search"));
}

// ── CompletionRequest ───────────────────────────────────────────────────────

#[test]
fn completion_request_construction() {
    let req = CompletionRequest {
        model: "gpt-4".into(),
        messages: vec![system("Be concise."), user("Hi")],
        max_tokens: Some(100),
        temperature: Some(0.7),
        stream: false,
        tools: None,
        tool_choice: None,
    };
    assert_eq!(req.model, "gpt-4");
    assert_eq!(req.messages.len(), 2);
}

#[test]
fn completion_request_tools_omitted_when_none() {
    let req = CompletionRequest {
        model: "test".into(),
        messages: vec![user("hi")],
        max_tokens: None,
        temperature: None,
        stream: false,
        tools: None,
        tool_choice: None,
    };
    let json = serde_json::to_value(&req).unwrap();
    assert!(json.get("tools").is_none());
    assert!(json.get("tool_choice").is_none());
}

// ── CompletionResponse ──────────────────────────────────────────────────────

#[test]
fn has_tool_calls_true() {
    let resp = CompletionResponse {
        message: rskit_llm::AssistantMessage {
            content: vec![],
            tool_calls: vec![ToolCall {
                id: "call_1".into(),
                call_type: "function".into(),
                function: FunctionCall {
                    name: "f".into(),
                    arguments: "{}".into(),
                },
            }],
            usage: None,
        },
        model: "test".into(),
        usage: Usage {
            input_tokens: 10,
            output_tokens: 5,
        },
        stop_reason: Some(rskit_llm::StopReason::ToolUse),
    };
    assert!(resp.has_tool_calls());
}

#[test]
fn has_tool_calls_false_when_empty() {
    let resp = CompletionResponse {
        message: rskit_llm::AssistantMessage {
            content: vec![ContentBlock::Text {
                text: "done".into(),
            }],
            tool_calls: vec![],
            usage: None,
        },
        model: "test".into(),
        usage: Usage {
            input_tokens: 10,
            output_tokens: 5,
        },
        stop_reason: Some(rskit_llm::StopReason::EndTurn),
    };
    assert!(!resp.has_tool_calls());
}

#[test]
fn completion_response_text() {
    let resp = CompletionResponse {
        message: rskit_llm::AssistantMessage {
            content: vec![ContentBlock::Text {
                text: "Hello!".into(),
            }],
            tool_calls: vec![],
            usage: None,
        },
        model: "test".into(),
        usage: Usage {
            input_tokens: 1,
            output_tokens: 1,
        },
        stop_reason: None,
    };
    assert_eq!(resp.text(), "Hello!");
}
