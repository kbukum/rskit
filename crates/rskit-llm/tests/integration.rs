use std::time::Duration;

use rskit_llm::{AnthropicConfig, ChatMessage, CompletionRequest, OpenAiConfig, Role};

// ── Role enum ───────────────────────────────────────────────────────────────

#[test]
fn role_variants_exist() {
    let system = Role::System;
    let user = Role::User;
    let assistant = Role::Assistant;
    assert_eq!(system, Role::System);
    assert_eq!(user, Role::User);
    assert_eq!(assistant, Role::Assistant);
}

#[test]
fn role_serde_roundtrip() {
    let role = Role::User;
    let json = serde_json::to_string(&role).unwrap();
    let back: Role = serde_json::from_str(&json).unwrap();
    assert_eq!(back, Role::User);
}

// ── ChatMessage ─────────────────────────────────────────────────────────────

#[test]
fn chat_message_construction() {
    let msg = ChatMessage {
        role: Role::User,
        content: "Hello!".into(),
    };
    assert_eq!(msg.role, Role::User);
    assert_eq!(msg.content, "Hello!");
}

#[test]
fn chat_message_serde_roundtrip() {
    let msg = ChatMessage {
        role: Role::System,
        content: "You are helpful.".into(),
    };
    let json = serde_json::to_string(&msg).unwrap();
    let back: ChatMessage = serde_json::from_str(&json).unwrap();
    assert_eq!(back.role, Role::System);
    assert_eq!(back.content, "You are helpful.");
}

// ── CompletionRequest ───────────────────────────────────────────────────────

#[test]
fn completion_request_construction() {
    let req = CompletionRequest {
        model: "gpt-4".into(),
        messages: vec![
            ChatMessage {
                role: Role::System,
                content: "Be concise.".into(),
            },
            ChatMessage {
                role: Role::User,
                content: "Hi".into(),
            },
        ],
        max_tokens: Some(100),
        temperature: Some(0.7),
        stream: false,
    };
    assert_eq!(req.model, "gpt-4");
    assert_eq!(req.messages.len(), 2);
    assert_eq!(req.max_tokens, Some(100));
    assert!(!req.stream);
}

#[test]
fn completion_request_serde_roundtrip() {
    let req = CompletionRequest {
        model: "claude-3".into(),
        messages: vec![ChatMessage {
            role: Role::User,
            content: "test".into(),
        }],
        max_tokens: None,
        temperature: None,
        stream: true,
    };
    let json = serde_json::to_string(&req).unwrap();
    let back: CompletionRequest = serde_json::from_str(&json).unwrap();
    assert_eq!(back.model, "claude-3");
    assert!(back.stream);
    assert!(back.max_tokens.is_none());
}

// ── OpenAiConfig ────────────────────────────────────────────────────────────

#[test]
fn openai_config_deserialise_with_defaults() {
    let json = r#"{"api_key":"sk-test"}"#;
    let cfg: OpenAiConfig = serde_json::from_str(json).unwrap();
    assert_eq!(cfg.api_key, "sk-test");
    assert_eq!(cfg.base_url, "https://api.openai.com/v1");
    assert_eq!(cfg.timeout, Duration::from_secs(30));
    assert_eq!(cfg.max_retries, 3);
}

#[test]
fn openai_config_custom_base_url() {
    let json =
        r#"{"api_key":"sk-test","base_url":"http://localhost:8080","timeout":60,"max_retries":1}"#;
    let cfg: OpenAiConfig = serde_json::from_str(json).unwrap();
    assert_eq!(cfg.base_url, "http://localhost:8080");
    assert_eq!(cfg.timeout, Duration::from_secs(60));
    assert_eq!(cfg.max_retries, 1);
}

// ── AnthropicConfig ─────────────────────────────────────────────────────────

#[test]
fn anthropic_config_deserialise_with_defaults() {
    let json = r#"{"api_key":"sk-ant-test"}"#;
    let cfg: AnthropicConfig = serde_json::from_str(json).unwrap();
    assert_eq!(cfg.api_key, "sk-ant-test");
    assert_eq!(cfg.base_url, "https://api.anthropic.com");
    assert_eq!(cfg.version, "2023-06-01");
    assert_eq!(cfg.timeout, Duration::from_secs(30));
    assert_eq!(cfg.max_retries, 3);
}

#[test]
fn anthropic_config_custom_values() {
    let json = r#"{"api_key":"key","base_url":"http://proxy","version":"2024-01-01","timeout":10,"max_retries":0}"#;
    let cfg: AnthropicConfig = serde_json::from_str(json).unwrap();
    assert_eq!(cfg.base_url, "http://proxy");
    assert_eq!(cfg.version, "2024-01-01");
    assert_eq!(cfg.timeout, Duration::from_secs(10));
    assert_eq!(cfg.max_retries, 0);
}

// ── Provider construction ───────────────────────────────────────────────────

#[test]
fn openai_provider_constructs_with_valid_config() {
    let cfg = OpenAiConfig {
        api_key: "sk-fake".into(),
        base_url: "https://api.openai.com/v1".into(),
        timeout: Duration::from_secs(10),
        max_retries: 1,
    };
    let provider = rskit_llm::OpenAiProvider::new(cfg);
    assert!(provider.is_ok());
}

#[test]
fn anthropic_provider_constructs_with_valid_config() {
    let cfg = AnthropicConfig {
        api_key: "sk-ant-fake".into(),
        base_url: "https://api.anthropic.com".into(),
        version: "2023-06-01".into(),
        timeout: Duration::from_secs(10),
        max_retries: 1,
    };
    let provider = rskit_llm::AnthropicProvider::new(cfg);
    assert!(provider.is_ok());
}

// ── API call tests (require live API keys) ──────────────────────────────────

#[tokio::test]
#[ignore = "requires OpenAI API key"]
async fn openai_complete_request() {
    use rskit_llm::LlmProvider;

    let cfg: OpenAiConfig = serde_json::from_str(r#"{"api_key":"sk-real-key"}"#).unwrap();
    let provider = rskit_llm::OpenAiProvider::new(cfg).unwrap();
    let req = CompletionRequest {
        model: "gpt-4o-mini".into(),
        messages: vec![ChatMessage {
            role: Role::User,
            content: "Say hello.".into(),
        }],
        max_tokens: Some(10),
        temperature: Some(0.0),
        stream: false,
    };
    let resp = provider.complete(req).await.unwrap();
    assert!(!resp.content.is_empty());
}

#[tokio::test]
#[ignore = "requires Anthropic API key"]
async fn anthropic_complete_request() {
    use rskit_llm::LlmProvider;

    let cfg: AnthropicConfig = serde_json::from_str(r#"{"api_key":"sk-ant-real-key"}"#).unwrap();
    let provider = rskit_llm::AnthropicProvider::new(cfg).unwrap();
    let req = CompletionRequest {
        model: "claude-3-haiku-20240307".into(),
        messages: vec![ChatMessage {
            role: Role::User,
            content: "Say hello.".into(),
        }],
        max_tokens: Some(10),
        temperature: Some(0.0),
        stream: false,
    };
    let resp = provider.complete(req).await.unwrap();
    assert!(!resp.content.is_empty());
}
