use serde_json::json;

use super::dialect::GeminiDialect;
use crate::common;

struct FixtureCase<'a> {
    name: &'a str,
    body: &'a str,
    expected: Vec<rskit_ai::ToolUseBlock>,
}

fn streamed_tool_uses(events: &[&[u8]]) -> Vec<rskit_ai::ToolUseBlock> {
    let chunks = events
        .iter()
        .map(|event| GeminiDialect::parse_stream_chunk(event).unwrap())
        .collect::<Vec<_>>();
    common::accumulate_tool_uses(chunks).unwrap()
}

#[test]
fn parses_non_streaming_fixtures() {
    let cases = [
        FixtureCase {
            name: "single tool call",
            body: r#"{
                "candidates": [{
                    "content": {
                        "parts": [{"functionCall": {"name": "get_weather", "args": {"location": "NYC"}}}],
                        "role": "model"
                    },
                    "finishReason": "TOOL_USE"
                }],
                "usageMetadata": {"promptTokenCount": 5, "candidatesTokenCount": 2}
            }"#,
            expected: vec![rskit_ai::ToolUseBlock {
                id: "tool_call_0".into(),
                name: "get_weather".into(),
                input: json!({"location": "NYC"}).as_object().cloned().unwrap(),
            }],
        },
        FixtureCase {
            name: "multi tool response",
            body: r#"{
                "candidates": [{
                    "content": {
                        "parts": [
                            {"functionCall": {"name": "get_weather", "args": {"location": "NYC"}}},
                            {"functionCall": {"name": "get_time", "args": {"timezone": "UTC"}}}
                        ],
                        "role": "model"
                    },
                    "finishReason": "TOOL_USE"
                }],
                "usageMetadata": {"promptTokenCount": 5, "candidatesTokenCount": 2}
            }"#,
            expected: vec![
                rskit_ai::ToolUseBlock {
                    id: "tool_call_0".into(),
                    name: "get_weather".into(),
                    input: json!({"location": "NYC"}).as_object().cloned().unwrap(),
                },
                rskit_ai::ToolUseBlock {
                    id: "tool_call_1".into(),
                    name: "get_time".into(),
                    input: json!({"timezone": "UTC"}).as_object().cloned().unwrap(),
                },
            ],
        },
        FixtureCase {
            name: "empty args become empty object",
            body: r#"{
                "candidates": [{
                    "content": {
                        "parts": [{"functionCall": {"name": "ping", "args": {}}}],
                        "role": "model"
                    },
                    "finishReason": "TOOL_USE"
                }],
                "usageMetadata": {"promptTokenCount": 5, "candidatesTokenCount": 2}
            }"#,
            expected: vec![rskit_ai::ToolUseBlock {
                id: "tool_call_0".into(),
                name: "ping".into(),
                input: serde_json::Map::new(),
            }],
        },
        FixtureCase {
            name: "nested input",
            body: r#"{
                "candidates": [{
                    "content": {
                        "parts": [{
                            "functionCall": {
                                "name": "search",
                                "args": {
                                    "query": {"city": "NYC", "filters": ["alerts", "daily"]},
                                    "limit": 3
                                }
                            }
                        }],
                        "role": "model"
                    },
                    "finishReason": "TOOL_USE"
                }],
                "usageMetadata": {"promptTokenCount": 5, "candidatesTokenCount": 2}
            }"#,
            expected: vec![rskit_ai::ToolUseBlock {
                id: "tool_call_0".into(),
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
        let response = GeminiDialect::parse_response(case.body, "gemini-2.5-flash").unwrap();
        assert_eq!(response.message.tool_calls, case.expected, "{}", case.name);
    }
}

#[test]
fn parses_streaming_deltas_into_tool_use_blocks() {
    let expected = vec![rskit_ai::ToolUseBlock {
        id: "tool_call_0".into(),
        name: "get_weather".into(),
        input: json!({"location": "NYC", "unit": "c"})
            .as_object()
            .cloned()
            .unwrap(),
    }];

    let actual = streamed_tool_uses(&[
        br#"{"candidates":[{"content":{"parts":[{"functionCall":{"name":"get_weather","args":{"location":"NYC","unit":"c"}}}],"role":"model"}}]}"#,
        br#"{"candidates":[{"content":{"parts":[],"role":"model"},"finishReason":"TOOL_USE"}]}"#,
    ]);

    assert_eq!(actual, expected);
}
