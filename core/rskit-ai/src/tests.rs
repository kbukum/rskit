use super::*;

#[test]
fn content_part_serializes_tool_result_alias() {
    let block = ContentPart::ToolResult {
        id: "call-1".into(),
        content: "ok".into(),
        is_error: false,
    };
    let json = serde_json::to_value(&block).unwrap();
    assert_eq!(json["type"], "tool_result");
    assert_eq!(json["id"], "call-1");

    let legacy = serde_json::json!({
        "type": "tool_result",
        "tool_use_id": "call-2",
        "content": "ok"
    });
    let decoded: ContentPart = serde_json::from_value(legacy).unwrap();
    assert!(matches!(decoded, ContentPart::ToolResult { id, .. } if id == "call-2"));
}

fn assert_stream_event<T: StreamEvent>() {}

#[test]
fn stream_event_types_implement_trait_and_report_locked_names() {
    assert_stream_event::<MessageStart>();
    assert_stream_event::<TextDelta>();
    assert_stream_event::<ReasoningDelta>();
    assert_stream_event::<ToolUseStart>();
    assert_stream_event::<ToolUseDelta>();
    assert_stream_event::<ToolUseStop>();
    assert_stream_event::<MessageStop>();
    assert_stream_event::<UsageDelta>();
    assert_stream_event::<ErrorEvent>();

    let events: [StreamEventRef; 9] = [
        std::sync::Arc::new(MessageStart {
            role: Role::Assistant,
            model: "model".into(),
            request_id: Some("req-1".into()),
        }),
        std::sync::Arc::new(TextDelta {
            text: "text".into(),
        }),
        std::sync::Arc::new(ReasoningDelta {
            text: "think".into(),
        }),
        std::sync::Arc::new(ToolUseStart {
            id: "call-1".into(),
            name: "search".into(),
        }),
        std::sync::Arc::new(ToolUseDelta {
            id: "call-1".into(),
            input_delta: "{\"q\"".into(),
        }),
        std::sync::Arc::new(ToolUseStop {
            id: "call-1".into(),
        }),
        std::sync::Arc::new(MessageStop {
            finish_reason: FinishReason::Stop,
        }),
        std::sync::Arc::new(UsageDelta {
            usage: Usage {
                input_tokens: 1,
                output_tokens: 2,
                cached_tokens: 3,
                reasoning_tokens: 4,
            },
        }),
        std::sync::Arc::new(ErrorEvent {
            message: "boom".into(),
            code: Some("provider_error".into()),
        }),
    ];
    let wire_names = events
        .into_iter()
        .map(|event| event.event_type().to_owned())
        .collect::<Vec<_>>();

    assert_eq!(
        wire_names,
        [
            "message.start",
            "text.delta",
            "reasoning.delta",
            "tool_use.start",
            "tool_use.delta",
            "tool_use.stop",
            "message.stop",
            "usage.delta",
            "error",
        ]
    );
}

#[test]
fn provider_custom_round_trips() {
    let provider = Provider::Custom("private".into());
    let json = serde_json::to_string(&provider).unwrap();
    let decoded: Provider = serde_json::from_str(&json).unwrap();
    assert_eq!(decoded, provider);
}

#[test]
fn model_serializes_capabilities_roundtrip() {
    let model = Model {
        name: "gpt-4o".into(),
        provider: Provider::OpenAI,
        version: Some("2024-08-06".into()),
        capabilities: Capabilities {
            streaming: true,
            vision: true,
            max_input_tokens: Some(128_000),
            ..Default::default()
        },
    };
    let json = serde_json::to_string(&model).unwrap();
    assert!(json.contains("max_input_tokens"));
    let decoded: Model = serde_json::from_str(&json).unwrap();
    assert_eq!(decoded, model);
}

#[test]
fn budget_roundtrips_and_errors_are_typed() {
    let budget = Budget {
        max_tokens: Some(10),
        max_calls: Some(2),
        max_cost: None,
        wall_clock: Some(60),
    };
    let json = serde_json::to_string(&budget).unwrap();
    let decoded: Budget = serde_json::from_str(&json).unwrap();
    assert_eq!(decoded, budget);

    let err = GenAiError::BudgetExceeded(BudgetExceededReason::Tokens);
    assert_eq!(err.to_string(), "budget exceeded: Tokens");
}

#[test]
fn semconv_keys_and_operations_are_locked() {
    assert_eq!(
        [
            semconv::SYSTEM,
            semconv::OPERATION_NAME,
            semconv::REQUEST_ID,
            semconv::REQUEST_MODEL,
            semconv::REQUEST_MODEL_VERSION,
            semconv::REQUEST_MAX_TOKENS,
            semconv::REQUEST_TEMPERATURE,
            semconv::RESPONSE_MODEL,
            semconv::RESPONSE_FINISH_REASON,
            semconv::TOOL_NAME,
            semconv::USAGE_INPUT_TOKENS,
            semconv::USAGE_OUTPUT_TOKENS,
            semconv::USAGE_CACHED_TOKENS,
            semconv::USAGE_REASONING_TOKENS,
        ],
        [
            "gen_ai.system",
            "gen_ai.operation.name",
            "gen_ai.request.id",
            "gen_ai.request.model",
            "gen_ai.request.model.version",
            "gen_ai.request.max_tokens",
            "gen_ai.request.temperature",
            "gen_ai.response.model",
            "gen_ai.response.finish_reason",
            "gen_ai.tool.name",
            "gen_ai.usage.input_tokens",
            "gen_ai.usage.output_tokens",
            "gen_ai.usage.cached_tokens",
            "gen_ai.usage.reasoning_tokens",
        ]
    );
    let operations = [
        (semconv::Operation::Chat, "chat"),
        (semconv::Operation::TextCompletion, "text_completion"),
        (semconv::Operation::Embedding, "embeddings"),
        (semconv::Operation::AgentTurn, "agent.turn"),
        (semconv::Operation::LlmCall, "llm.call"),
        (semconv::Operation::ToolCall, "tool.call"),
        (semconv::Operation::McpRequest, "mcp.request"),
        (semconv::Operation::Stream, "stream"),
        (semconv::Operation::InferenceRequest, "inference.request"),
    ];
    for (operation, name) in operations {
        assert_eq!(operation.as_str(), name);
        assert_eq!(
            semconv::Operation::from_operation_name(name),
            Some(operation)
        );
    }
    assert_eq!(semconv::Operation::from_operation_name("predict"), None);
}
