use serde_json::json;

use super::dialect::AnthropicDialect;
use crate::common;

struct FixtureCase<'a> {
    name: &'a str,
    body: &'a str,
    expected: Vec<rskit_ai::ToolUseBlock>,
}

fn streamed_tool_uses(events: &[&[u8]]) -> Vec<rskit_ai::ToolUseBlock> {
    let chunks = events
        .iter()
        .map(|event| AnthropicDialect::parse_stream_chunk(event).unwrap())
        .collect::<Vec<_>>();
    common::accumulate_tool_uses(chunks).unwrap()
}

#[test]
fn parses_non_streaming_fixtures() {
    let cases = [
        FixtureCase {
            name: "single tool call",
            body: r#"{
                "model": "claude-sonnet-4-20250514",
                "content": [
                    {"type": "tool_use", "id": "toolu_1", "name": "get_weather", "input": {"location": "NYC"}}
                ],
                "usage": {"input_tokens": 8, "output_tokens": 3},
                "stop_reason": "tool_use"
            }"#,
            expected: vec![rskit_ai::ToolUseBlock {
                id: "toolu_1".into(),
                name: "get_weather".into(),
                input: json!({"location": "NYC"}).as_object().cloned().unwrap(),
            }],
        },
        FixtureCase {
            name: "multi tool response",
            body: r#"{
                "model": "claude-sonnet-4-20250514",
                "content": [
                    {"type": "tool_use", "id": "toolu_1", "name": "get_weather", "input": {"location": "NYC"}},
                    {"type": "tool_use", "id": "toolu_2", "name": "get_time", "input": {"timezone": "UTC"}}
                ],
                "usage": {"input_tokens": 8, "output_tokens": 3},
                "stop_reason": "tool_use"
            }"#,
            expected: vec![
                rskit_ai::ToolUseBlock {
                    id: "toolu_1".into(),
                    name: "get_weather".into(),
                    input: json!({"location": "NYC"}).as_object().cloned().unwrap(),
                },
                rskit_ai::ToolUseBlock {
                    id: "toolu_2".into(),
                    name: "get_time".into(),
                    input: json!({"timezone": "UTC"}).as_object().cloned().unwrap(),
                },
            ],
        },
        FixtureCase {
            name: "empty args become empty object",
            body: r#"{
                "model": "claude-sonnet-4-20250514",
                "content": [
                    {"type": "tool_use", "id": "toolu_1", "name": "ping", "input": {}}
                ],
                "usage": {"input_tokens": 8, "output_tokens": 3},
                "stop_reason": "tool_use"
            }"#,
            expected: vec![rskit_ai::ToolUseBlock {
                id: "toolu_1".into(),
                name: "ping".into(),
                input: serde_json::Map::new(),
            }],
        },
        FixtureCase {
            name: "nested input",
            body: r#"{
                "model": "claude-sonnet-4-20250514",
                "content": [
                    {
                        "type": "tool_use",
                        "id": "toolu_1",
                        "name": "search",
                        "input": {
                            "query": {"city": "NYC", "filters": ["alerts", "daily"]},
                            "limit": 3
                        }
                    }
                ],
                "usage": {"input_tokens": 8, "output_tokens": 3},
                "stop_reason": "tool_use"
            }"#,
            expected: vec![rskit_ai::ToolUseBlock {
                id: "toolu_1".into(),
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
        let response = AnthropicDialect::parse_response(case.body).unwrap();
        assert_eq!(response.message.tool_calls, case.expected, "{}", case.name);
    }
}

#[test]
fn parses_streaming_deltas_into_tool_use_blocks() {
    let expected = vec![rskit_ai::ToolUseBlock {
        id: "toolu_1".into(),
        name: "get_weather".into(),
        input: json!({"location": "NYC", "unit": "c"})
            .as_object()
            .cloned()
            .unwrap(),
    }];

    let actual = streamed_tool_uses(&[
        br#"{"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"toolu_1","name":"get_weather"}}"#,
        br#"{"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":"{\"location\":\"NY"}}"#,
        br#"{"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":"C\",\"unit\":\"c\"}"}}"#,
        br#"{"type":"message_stop"}"#,
    ]);

    assert_eq!(actual, expected);
}
