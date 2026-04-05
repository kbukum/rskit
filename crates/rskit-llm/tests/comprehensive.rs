use std::time::Duration;

use rskit_llm::{
    AnthropicConfig, AnthropicProvider, ChatMessage, CompletionRequest, CompletionResponse,
    LlmProvider, OpenAiConfig, OpenAiProvider, Role, TokenUsage,
};

// ═══════════════════════════════════════════════════════════════════════════
// Role enum — comprehensive tests
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn role_system_variant() {
    let r = Role::System;
    assert_eq!(r, Role::System);
}

#[test]
fn role_user_variant() {
    let r = Role::User;
    assert_eq!(r, Role::User);
}

#[test]
fn role_assistant_variant() {
    let r = Role::Assistant;
    assert_eq!(r, Role::Assistant);
}

#[test]
fn role_clone() {
    let r = Role::User;
    let r2 = r.clone();
    assert_eq!(r, r2);
}

#[test]
fn role_debug_format() {
    let r = Role::System;
    let dbg = format!("{:?}", r);
    assert!(dbg.contains("System"), "Debug format = {dbg}");
}

#[test]
fn role_serialize_all_variants() {
    for (role, expected) in [
        (Role::System, "\"System\""),
        (Role::User, "\"User\""),
        (Role::Assistant, "\"Assistant\""),
    ] {
        let json = serde_json::to_string(&role).unwrap();
        assert_eq!(json, expected, "serialized {role:?}");
    }
}

#[test]
fn role_deserialize_all_variants() {
    for (json_str, expected) in [
        ("\"System\"", Role::System),
        ("\"User\"", Role::User),
        ("\"Assistant\"", Role::Assistant),
    ] {
        let role: Role = serde_json::from_str(json_str).unwrap();
        assert_eq!(role, expected, "deserialized {json_str}");
    }
}

#[test]
fn role_deserialize_invalid() {
    let result = serde_json::from_str::<Role>("\"Admin\"");
    assert!(result.is_err(), "expected error for invalid role");
}

#[test]
fn role_serde_roundtrip_all() {
    for role in [Role::System, Role::User, Role::Assistant] {
        let json = serde_json::to_string(&role).unwrap();
        let back: Role = serde_json::from_str(&json).unwrap();
        assert_eq!(back, role);
    }
}

// ═══════════════════════════════════════════════════════════════════════════
// ChatMessage — construction, clone, serde
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn chat_message_basic_construction() {
    let msg = ChatMessage {
        role: Role::User,
        content: "Hello!".into(),
    };
    assert_eq!(msg.role, Role::User);
    assert_eq!(msg.content, "Hello!");
}

#[test]
fn chat_message_system() {
    let msg = ChatMessage {
        role: Role::System,
        content: "You are helpful.".into(),
    };
    assert_eq!(msg.role, Role::System);
}

#[test]
fn chat_message_assistant() {
    let msg = ChatMessage {
        role: Role::Assistant,
        content: "Sure, I can help.".into(),
    };
    assert_eq!(msg.role, Role::Assistant);
}

#[test]
fn chat_message_empty_content() {
    let msg = ChatMessage {
        role: Role::User,
        content: String::new(),
    };
    assert!(msg.content.is_empty());
}

#[test]
fn chat_message_large_content() {
    let big = "x".repeat(100_000);
    let msg = ChatMessage {
        role: Role::User,
        content: big.clone(),
    };
    assert_eq!(msg.content.len(), 100_000);
}

#[test]
fn chat_message_clone() {
    let msg = ChatMessage {
        role: Role::User,
        content: "clone me".into(),
    };
    let msg2 = msg.clone();
    assert_eq!(msg.content, msg2.content);
    assert_eq!(msg.role, msg2.role);
}

#[test]
fn chat_message_serialize() {
    let msg = ChatMessage {
        role: Role::User,
        content: "hi".into(),
    };
    let json = serde_json::to_value(&msg).unwrap();
    assert_eq!(json["role"], "User");
    assert_eq!(json["content"], "hi");
}

#[test]
fn chat_message_deserialize() {
    let json = r#"{"role":"Assistant","content":"ok"}"#;
    let msg: ChatMessage = serde_json::from_str(json).unwrap();
    assert_eq!(msg.role, Role::Assistant);
    assert_eq!(msg.content, "ok");
}

#[test]
fn chat_message_serde_roundtrip() {
    let msg = ChatMessage {
        role: Role::System,
        content: "Round trip test.".into(),
    };
    let json = serde_json::to_string(&msg).unwrap();
    let back: ChatMessage = serde_json::from_str(&json).unwrap();
    assert_eq!(back.role, msg.role);
    assert_eq!(back.content, msg.content);
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
    };
    assert!(req.model.is_empty());
    assert!(req.messages.is_empty());
    assert!(req.max_tokens.is_none());
    assert!(req.temperature.is_none());
    assert!(!req.stream);
}

#[test]
fn completion_request_all_fields() {
    let req = CompletionRequest {
        model: "gpt-4o".into(),
        messages: vec![
            ChatMessage {
                role: Role::System,
                content: "Be concise.".into(),
            },
            ChatMessage {
                role: Role::User,
                content: "Explain rust.".into(),
            },
            ChatMessage {
                role: Role::Assistant,
                content: "Rust is a systems language.".into(),
            },
        ],
        max_tokens: Some(4096),
        temperature: Some(0.8),
        stream: true,
    };
    assert_eq!(req.model, "gpt-4o");
    assert_eq!(req.messages.len(), 3);
    assert_eq!(req.max_tokens, Some(4096));
    assert_eq!(req.temperature, Some(0.8));
    assert!(req.stream);
}

#[test]
fn completion_request_zero_temperature() {
    let req = CompletionRequest {
        model: "m".into(),
        messages: vec![],
        max_tokens: None,
        temperature: Some(0.0),
        stream: false,
    };
    assert_eq!(req.temperature, Some(0.0));
}

#[test]
fn completion_request_serialize() {
    let req = CompletionRequest {
        model: "gpt-4".into(),
        messages: vec![ChatMessage {
            role: Role::User,
            content: "hi".into(),
        }],
        max_tokens: Some(100),
        temperature: Some(0.5),
        stream: false,
    };
    let json = serde_json::to_value(&req).unwrap();
    assert_eq!(json["model"], "gpt-4");
    assert_eq!(json["max_tokens"], 100);
    assert_eq!(json["temperature"], 0.5);
    assert_eq!(json["stream"], false);
    assert_eq!(json["messages"][0]["role"], "User");
}

#[test]
fn completion_request_serialize_none_fields() {
    let req = CompletionRequest {
        model: "m".into(),
        messages: vec![],
        max_tokens: None,
        temperature: None,
        stream: false,
    };
    let json = serde_json::to_value(&req).unwrap();
    // None serializes as null in serde_json
    assert!(json["max_tokens"].is_null());
    assert!(json["temperature"].is_null());
}

#[test]
fn completion_request_serde_roundtrip() {
    let req = CompletionRequest {
        model: "claude-3".into(),
        messages: vec![
            ChatMessage {
                role: Role::System,
                content: "sys".into(),
            },
            ChatMessage {
                role: Role::User,
                content: "usr".into(),
            },
        ],
        max_tokens: Some(500),
        temperature: Some(0.9),
        stream: true,
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
// CompletionResponse — deserialization
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn completion_response_deserialize() {
    let json = r#"{
        "id": "resp-1",
        "content": "Hello world!",
        "model": "gpt-4",
        "usage": {
            "input_tokens": 10,
            "output_tokens": 5
        }
    }"#;
    let resp: CompletionResponse = serde_json::from_str(json).unwrap();
    assert_eq!(resp.id, "resp-1");
    assert_eq!(resp.content, "Hello world!");
    assert_eq!(resp.model, "gpt-4");
    assert_eq!(resp.usage.input_tokens, 10);
    assert_eq!(resp.usage.output_tokens, 5);
}

#[test]
fn completion_response_empty_content() {
    let json = r#"{
        "id": "resp-2",
        "content": "",
        "model": "m",
        "usage": {"input_tokens": 0, "output_tokens": 0}
    }"#;
    let resp: CompletionResponse = serde_json::from_str(json).unwrap();
    assert!(resp.content.is_empty());
}

#[test]
fn completion_response_clone() {
    let json = r#"{
        "id": "resp-3",
        "content": "test",
        "model": "m",
        "usage": {"input_tokens": 1, "output_tokens": 2}
    }"#;
    let resp: CompletionResponse = serde_json::from_str(json).unwrap();
    let resp2 = resp.clone();
    assert_eq!(resp.id, resp2.id);
    assert_eq!(resp.content, resp2.content);
}

#[test]
fn completion_response_missing_field_errors() {
    // Missing required "id" field
    let json = r#"{"content":"hi","model":"m","usage":{"input_tokens":0,"output_tokens":0}}"#;
    let result = serde_json::from_str::<CompletionResponse>(json);
    assert!(result.is_err(), "expected error when id is missing");
}

// ═══════════════════════════════════════════════════════════════════════════
// TokenUsage — deserialization, edge cases
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn token_usage_deserialize() {
    let json = r#"{"input_tokens": 100, "output_tokens": 50}"#;
    let usage: TokenUsage = serde_json::from_str(json).unwrap();
    assert_eq!(usage.input_tokens, 100);
    assert_eq!(usage.output_tokens, 50);
}

#[test]
fn token_usage_zero_values() {
    let json = r#"{"input_tokens": 0, "output_tokens": 0}"#;
    let usage: TokenUsage = serde_json::from_str(json).unwrap();
    assert_eq!(usage.input_tokens, 0);
    assert_eq!(usage.output_tokens, 0);
}

#[test]
fn token_usage_large_values() {
    let json = r#"{"input_tokens": 1000000, "output_tokens": 500000}"#;
    let usage: TokenUsage = serde_json::from_str(json).unwrap();
    assert_eq!(usage.input_tokens, 1_000_000);
    assert_eq!(usage.output_tokens, 500_000);
}

#[test]
fn token_usage_clone() {
    let json = r#"{"input_tokens": 10, "output_tokens": 5}"#;
    let usage: TokenUsage = serde_json::from_str(json).unwrap();
    let usage2 = usage.clone();
    assert_eq!(usage.input_tokens, usage2.input_tokens);
    assert_eq!(usage.output_tokens, usage2.output_tokens);
}

#[test]
fn token_usage_debug() {
    let json = r#"{"input_tokens": 10, "output_tokens": 5}"#;
    let usage: TokenUsage = serde_json::from_str(json).unwrap();
    let dbg = format!("{:?}", usage);
    assert!(dbg.contains("10"));
    assert!(dbg.contains("5"));
}

// ═══════════════════════════════════════════════════════════════════════════
// OpenAiConfig — construction, serde, defaults
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn openai_config_minimal_json() {
    let json = r#"{"api_key":"sk-test"}"#;
    let cfg: OpenAiConfig = serde_json::from_str(json).unwrap();
    assert_eq!(cfg.api_key, "sk-test");
    assert_eq!(cfg.base_url, "https://api.openai.com/v1");
    assert_eq!(cfg.timeout, Duration::from_secs(30));
    assert_eq!(cfg.max_retries, 3);
}

#[test]
fn openai_config_all_fields() {
    let json = r#"{
        "api_key": "sk-test",
        "base_url": "http://localhost:8080",
        "timeout": 60,
        "max_retries": 5
    }"#;
    let cfg: OpenAiConfig = serde_json::from_str(json).unwrap();
    assert_eq!(cfg.base_url, "http://localhost:8080");
    assert_eq!(cfg.timeout, Duration::from_secs(60));
    assert_eq!(cfg.max_retries, 5);
}

#[test]
fn openai_config_zero_retries() {
    let json = r#"{"api_key":"k","max_retries":0}"#;
    let cfg: OpenAiConfig = serde_json::from_str(json).unwrap();
    assert_eq!(cfg.max_retries, 0);
}

#[test]
fn openai_config_clone() {
    let json = r#"{"api_key":"sk-clone"}"#;
    let cfg: OpenAiConfig = serde_json::from_str(json).unwrap();
    let cfg2 = cfg.clone();
    assert_eq!(cfg.api_key, cfg2.api_key);
    assert_eq!(cfg.base_url, cfg2.base_url);
}

// ═══════════════════════════════════════════════════════════════════════════
// AnthropicConfig — construction, serde, defaults
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn anthropic_config_minimal_json() {
    let json = r#"{"api_key":"sk-ant"}"#;
    let cfg: AnthropicConfig = serde_json::from_str(json).unwrap();
    assert_eq!(cfg.api_key, "sk-ant");
    assert_eq!(cfg.base_url, "https://api.anthropic.com");
    assert_eq!(cfg.version, "2023-06-01");
    assert_eq!(cfg.timeout, Duration::from_secs(30));
    assert_eq!(cfg.max_retries, 3);
}

#[test]
fn anthropic_config_all_fields() {
    let json = r#"{
        "api_key": "sk-ant",
        "base_url": "http://proxy",
        "version": "2024-01-01",
        "timeout": 10,
        "max_retries": 0
    }"#;
    let cfg: AnthropicConfig = serde_json::from_str(json).unwrap();
    assert_eq!(cfg.base_url, "http://proxy");
    assert_eq!(cfg.version, "2024-01-01");
    assert_eq!(cfg.timeout, Duration::from_secs(10));
    assert_eq!(cfg.max_retries, 0);
}

#[test]
fn anthropic_config_clone() {
    let json = r#"{"api_key":"sk-ant-clone"}"#;
    let cfg: AnthropicConfig = serde_json::from_str(json).unwrap();
    let cfg2 = cfg.clone();
    assert_eq!(cfg.api_key, cfg2.api_key);
    assert_eq!(cfg.version, cfg2.version);
}

// ═══════════════════════════════════════════════════════════════════════════
// Provider construction
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn openai_provider_new_ok() {
    let cfg = OpenAiConfig {
        api_key: "sk-fake".into(),
        base_url: "https://api.openai.com/v1".into(),
        timeout: Duration::from_secs(10),
        max_retries: 1,
    };
    assert!(OpenAiProvider::new(cfg).is_ok());
}

#[test]
fn anthropic_provider_new_ok() {
    let cfg = AnthropicConfig {
        api_key: "sk-ant-fake".into(),
        base_url: "https://api.anthropic.com".into(),
        version: "2023-06-01".into(),
        timeout: Duration::from_secs(10),
        max_retries: 1,
    };
    assert!(AnthropicProvider::new(cfg).is_ok());
}

#[test]
fn openai_provider_with_custom_url() {
    let cfg = OpenAiConfig {
        api_key: "sk-test".into(),
        base_url: "http://localhost:8080/v1".into(),
        timeout: Duration::from_secs(5),
        max_retries: 0,
    };
    let provider = OpenAiProvider::new(cfg);
    assert!(provider.is_ok());
}

#[test]
fn anthropic_provider_with_custom_url() {
    let cfg = AnthropicConfig {
        api_key: "sk-ant".into(),
        base_url: "http://localhost:9090".into(),
        version: "2024-01-01".into(),
        timeout: Duration::from_secs(5),
        max_retries: 0,
    };
    let provider = AnthropicProvider::new(cfg);
    assert!(provider.is_ok());
}

// ═══════════════════════════════════════════════════════════════════════════
// Trait object creation — dyn LlmProvider
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn openai_provider_as_trait_object() {
    let cfg = OpenAiConfig {
        api_key: "sk-test".into(),
        base_url: "https://api.openai.com/v1".into(),
        timeout: Duration::from_secs(10),
        max_retries: 1,
    };
    let provider = OpenAiProvider::new(cfg).unwrap();
    // Verify we can create a trait object (Send + Sync)
    let _boxed: Box<dyn LlmProvider> = Box::new(provider);
}

#[test]
fn anthropic_provider_as_trait_object() {
    let cfg = AnthropicConfig {
        api_key: "sk-ant".into(),
        base_url: "https://api.anthropic.com".into(),
        version: "2023-06-01".into(),
        timeout: Duration::from_secs(10),
        max_retries: 1,
    };
    let provider = AnthropicProvider::new(cfg).unwrap();
    let _boxed: Box<dyn LlmProvider> = Box::new(provider);
}

// ═══════════════════════════════════════════════════════════════════════════
// Anthropic embed returns error
// ═══════════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn anthropic_embed_unsupported() {
    let cfg = AnthropicConfig {
        api_key: "sk-ant".into(),
        base_url: "https://api.anthropic.com".into(),
        version: "2023-06-01".into(),
        timeout: Duration::from_secs(10),
        max_retries: 0,
    };
    let provider = AnthropicProvider::new(cfg).unwrap();
    let result = provider.embed(vec!["test".into()]).await;
    assert!(result.is_err(), "Anthropic embed should return error");
    let err = result.unwrap_err();
    let msg = format!("{}", err);
    assert!(
        msg.contains("not support") || msg.contains("embeddings"),
        "error should mention embeddings: {msg}"
    );
}

// ═══════════════════════════════════════════════════════════════════════════
// Multi-message conversation construction
// ═══════════════════════════════════════════════════════════════════════════

#[test]
fn multi_turn_conversation() {
    let req = CompletionRequest {
        model: "gpt-4".into(),
        messages: vec![
            ChatMessage {
                role: Role::System,
                content: "You are a math tutor.".into(),
            },
            ChatMessage {
                role: Role::User,
                content: "What is 2+2?".into(),
            },
            ChatMessage {
                role: Role::Assistant,
                content: "4".into(),
            },
            ChatMessage {
                role: Role::User,
                content: "And 3+3?".into(),
            },
        ],
        max_tokens: Some(50),
        temperature: Some(0.0),
        stream: false,
    };
    assert_eq!(req.messages.len(), 4);
    assert_eq!(req.messages[0].role, Role::System);
    assert_eq!(req.messages[3].role, Role::User);
    assert_eq!(req.messages[3].content, "And 3+3?");
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
    };
    let val = serde_json::to_value(&req).unwrap();
    // Check that serde field names are as expected
    assert!(val.get("model").is_some());
    assert!(val.get("messages").is_some());
    assert!(val.get("max_tokens").is_some());
    assert!(val.get("temperature").is_some());
    assert!(val.get("stream").is_some());
}

#[test]
fn token_usage_json_field_names() {
    let json = r#"{"input_tokens": 1, "output_tokens": 2}"#;
    let usage: TokenUsage = serde_json::from_str(json).unwrap();
    assert_eq!(usage.input_tokens, 1);
    assert_eq!(usage.output_tokens, 2);
}

#[test]
fn completion_response_json_field_names() {
    let json = serde_json::json!({
        "id": "test",
        "content": "hello",
        "model": "m",
        "usage": {
            "input_tokens": 0,
            "output_tokens": 0
        }
    });
    let resp: CompletionResponse = serde_json::from_value(json).unwrap();
    assert_eq!(resp.id, "test");
}
