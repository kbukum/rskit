use super::*;
use rskit_hook::{Event, EventType};
use rskit_llm::types::{AssistantMessage, CompletionRequest, CompletionResponse};
use rskit_tool::ToolInput;

#[test]
fn test_pre_tool_call_event() {
    let event = PreToolCall {
        name: "calculator".to_string(),
        input: ToolInput::new(serde_json::json!({"x": 1})).unwrap(),
    };
    assert_eq!(event.event_type(), EventType::new("on_tool_call"));
    assert_eq!(event.name, "calculator");
}

#[test]
fn test_post_tool_call_event() {
    let event = PostToolCall {
        name: "calculator".to_string(),
        input: ToolInput::new(serde_json::json!({})).unwrap(),
        result: None,
        error: Some("timeout".to_string()),
    };
    assert_eq!(event.event_type(), EventType::new("on_tool_result"));
}

#[test]
fn test_pre_llm_call_event() {
    let event = PreLLMCall {
        request: CompletionRequest {
            model: "test".to_string(),
            messages: vec![],
            max_tokens: None,
            temperature: None,
            stream: false,
            tools: None,
            tool_choice: None,
        },
    };
    assert_eq!(event.event_type(), EventType::new("on_llm_call"));
}

#[test]
fn test_on_error_event() {
    let event = OnError {
        error: "boom".to_string(),
        source: "agent".to_string(),
    };
    assert_eq!(event.event_type(), EventType::new("on_error"));
}

#[test]
fn test_turn_start_event() {
    let event = TurnStart { turn: 0 };
    assert_eq!(event.event_type(), EventType::new("on_turn_start"));
    assert_eq!(event.turn, 0);
}

#[test]
fn test_turn_end_event() {
    use rskit_llm::types;
    let event = TurnEnd {
        turn: 3,
        message: AssistantMessage {
            content: types::text_content("done"),
            tool_calls: vec![],
            usage: None,
        },
    };
    assert_eq!(event.event_type(), EventType::new("on_turn_complete"));
}

#[test]
fn test_event_type_constants() {
    assert_eq!(pre_tool_call_type(), EventType::new("on_tool_call"));
    assert_eq!(post_tool_call_type(), EventType::new("on_tool_result"));
    assert_eq!(pre_llm_call_type(), EventType::new("on_llm_call"));
    assert_eq!(post_llm_call_type(), EventType::new("on_llm_response"));
    assert_eq!(on_error_type(), EventType::new("on_error"));
    assert_eq!(turn_start_type(), EventType::new("on_turn_start"));
    assert_eq!(turn_end_type(), EventType::new("on_turn_complete"));
    assert_eq!(on_event_type(), EventType::new("on_event"));
    assert_eq!(on_mcp_call_type(), EventType::new("on_mcp_call"));
    assert_eq!(on_mcp_result_type(), EventType::new("on_mcp_result"));
    assert_eq!(on_turn_complete_type(), EventType::new("on_turn_complete"));
}

#[test]
fn canonical_event_structs_expose_event_types_and_payloads() {
    use rskit_ai::{FinishReason, MessageStop};
    use rskit_llm::types;

    let stream = OnEvent {
        event: std::sync::Arc::new(MessageStop {
            finish_reason: FinishReason::Stop,
        }),
    };
    assert_eq!(stream.event_type(), EventType::new("on_event"));

    let mcp_call = OnMCPCall {
        method: "tools/call".to_string(),
    };
    assert_eq!(mcp_call.event_type(), EventType::new("on_mcp_call"));
    assert_eq!(mcp_call.method, "tools/call");

    let mcp_result = OnMCPResult {
        method: "tools/call".to_string(),
        result: Some(serde_json::json!({"ok": true})),
        error: None,
    };
    assert_eq!(mcp_result.event_type(), EventType::new("on_mcp_result"));
    assert_eq!(mcp_result.result.unwrap()["ok"], true);

    let response = CompletionResponse {
        message: AssistantMessage {
            content: types::text_content("ok"),
            tool_calls: vec![],
            usage: None,
        },
        model: "mock".to_string(),
        usage: types::Usage {
            input_tokens: 1,
            output_tokens: 1,
            cached_tokens: 0,
            reasoning_tokens: 0,
        },
        stop_reason: None,
    };
    let post = PostLLMCall {
        response,
        error: None,
    };
    assert_eq!(post.event_type(), EventType::new("on_llm_response"));
    assert_eq!(post.response.model, "mock");
}
