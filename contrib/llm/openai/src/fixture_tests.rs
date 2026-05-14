use serde_json::json;

use super::dialect::OpenAiDialect;
use rskit_llm_common as common;

struct FixtureCase<'a> {
    name: &'a str,
    body: &'a str,
    expected: Vec<rskit_ai::ToolUseBlock>,
}

fn streamed_tool_uses(events: &[&[u8]]) -> Vec<rskit_ai::ToolUseBlock> {
    let chunks = events
        .iter()
        .map(|event| OpenAiDialect::parse_stream_chunk(event).unwrap())
        .collect::<Vec<_>>();
    common::accumulate_tool_uses(chunks).unwrap()
}

#[test]
#[allow(clippy::too_many_lines)]
fn parses_non_streaming_fixtures() {
    let cases = [
        FixtureCase {
            name: "single tool call",
            body: r#"{
                "model": "gpt-4o",
                "choices": [{
                    "message": {
                        "content": null,
                        "tool_calls": [{
                            "id": "call_1",
                            "type": "function",
                            "function": {
                                "name": "get_weather",
                                "arguments": "{\"location\":\"NYC\"}"
                            }
                        }]
                    },
                    "finish_reason": "tool_calls"
                }],
                "usage": {"prompt_tokens": 10, "completion_tokens": 5}
            }"#,
            expected: vec![rskit_ai::ToolUseBlock {
                id: "call_1".into(),
                name: "get_weather".into(),
                input: json!({"location": "NYC"}).as_object().cloned().unwrap(),
            }],
        },
        FixtureCase {
            name: "multi tool response",
            body: r#"{
                "model": "gpt-4o",
                "choices": [{
                    "message": {
                        "content": null,
                        "tool_calls": [
                            {
                                "id": "call_1",
                                "type": "function",
                                "function": {
                                    "name": "get_weather",
                                    "arguments": "{\"location\":\"NYC\"}"
                                }
                            },
                            {
                                "id": "call_2",
                                "type": "function",
                                "function": {
                                    "name": "get_time",
                                    "arguments": "{\"timezone\":\"UTC\"}"
                                }
                            }
                        ]
                    },
                    "finish_reason": "tool_calls"
                }],
                "usage": {"prompt_tokens": 10, "completion_tokens": 5}
            }"#,
            expected: vec![
                rskit_ai::ToolUseBlock {
                    id: "call_1".into(),
                    name: "get_weather".into(),
                    input: json!({"location": "NYC"}).as_object().cloned().unwrap(),
                },
                rskit_ai::ToolUseBlock {
                    id: "call_2".into(),
                    name: "get_time".into(),
                    input: json!({"timezone": "UTC"}).as_object().cloned().unwrap(),
                },
            ],
        },
        FixtureCase {
            name: "empty args become empty object",
            body: r#"{
                "model": "gpt-4o",
                "choices": [{
                    "message": {
                        "content": null,
                        "tool_calls": [{
                            "id": "call_1",
                            "type": "function",
                            "function": {
                                "name": "ping",
                                "arguments": "{}"
                            }
                        }]
                    },
                    "finish_reason": "tool_calls"
                }],
                "usage": {"prompt_tokens": 10, "completion_tokens": 5}
            }"#,
            expected: vec![rskit_ai::ToolUseBlock {
                id: "call_1".into(),
                name: "ping".into(),
                input: serde_json::Map::new(),
            }],
        },
        FixtureCase {
            name: "nested input",
            body: r#"{
                "model": "gpt-4o",
                "choices": [{
                    "message": {
                        "content": null,
                        "tool_calls": [{
                            "id": "call_1",
                            "type": "function",
                            "function": {
                                "name": "search",
                                "arguments": "{\"query\":{\"city\":\"NYC\",\"filters\":[\"alerts\",\"daily\"]},\"limit\":3}"
                            }
                        }]
                    },
                    "finish_reason": "tool_calls"
                }],
                "usage": {"prompt_tokens": 10, "completion_tokens": 5}
            }"#,
            expected: vec![rskit_ai::ToolUseBlock {
                id: "call_1".into(),
                name: "search".into(),
                input: json!({
                    "query": {"city": "NYC", "filters": ["alerts", "daily"]},
                    "limit": 3
                })
                .as_object()
                .cloned()
                .unwrap(),
            }],
        },
    ];

    for case in cases {
        let response = OpenAiDialect::parse_response(case.body).unwrap();
        assert_eq!(response.message.tool_calls, case.expected, "{}", case.name);
    }
}

#[test]
fn parses_streaming_deltas_into_tool_use_blocks() {
    let expected = vec![rskit_ai::ToolUseBlock {
        id: "call_1".into(),
        name: "get_weather".into(),
        input: json!({"location": "NYC", "unit": "c"})
            .as_object()
            .cloned()
            .unwrap(),
    }];

    let actual = streamed_tool_uses(&[
        br#"{"choices":[{"delta":{"tool_calls":[{"index":0,"id":"call_1","type":"function","function":{"name":"get_weather","arguments":"{\"location\":\"NY"}}]},"finish_reason":null}]}"#,
        br#"{"choices":[{"delta":{"tool_calls":[{"index":0,"type":"function","function":{"arguments":"C\",\"unit\":\"c\"}"}}]},"finish_reason":null}]}"#,
        br#"{"choices":[{"delta":{},"finish_reason":"tool_calls"}]}"#,
    ]);

    assert_eq!(actual, expected);
}
