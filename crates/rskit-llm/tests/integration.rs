use std::time::Duration;

use rskit_anthropic::AnthropicConfig;
use rskit_llm::{CompletionRequest, Message, assistant, system, user};
use rskit_openai::OpenAiConfig;

// ── Message enum ────────────────────────────────────────────────────────────

#[test]
fn message_roles() {
    assert_eq!(user("hi").role(), "user");
    assert_eq!(assistant("hi").role(), "assistant");
    assert_eq!(system("hi").role(), "system");
}

#[test]
fn message_serde_roundtrip() {
    let msg = user("Hello!");
    let json = serde_json::to_string(&msg).unwrap();
    let back: Message = serde_json::from_str(&json).unwrap();
    assert_eq!(back.role(), "user");
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
    assert_eq!(req.max_tokens, Some(100));
    assert!(!req.stream);
}

#[test]
fn completion_request_serde_roundtrip() {
    let req = CompletionRequest {
        model: "claude-3".into(),
        messages: vec![user("test")],
        max_tokens: None,
        temperature: None,
        stream: true,
        tools: None,
        tool_choice: None,
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
    let provider = rskit_openai::OpenAiProvider::new(cfg);
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
    let provider = rskit_anthropic::AnthropicProvider::new(cfg);
    assert!(provider.is_ok());
}

// ── API call tests (require live API keys) ──────────────────────────────────

#[tokio::test]
#[ignore = "requires OpenAI API key"]
async fn openai_complete_request() {
    use rskit_llm::LlmProvider;

    let cfg: OpenAiConfig = serde_json::from_str(r#"{"api_key":"sk-real-key"}"#).unwrap();
    let provider = rskit_openai::OpenAiProvider::new(cfg).unwrap();
    let req = CompletionRequest {
        model: "gpt-4o-mini".into(),
        messages: vec![user("Say hello.")],
        max_tokens: Some(10),
        temperature: Some(0.0),
        stream: false,
        tools: None,
        tool_choice: None,
    };
    let resp = provider.complete(req).await.unwrap();
    assert!(!resp.text().is_empty());
}

#[tokio::test]
#[ignore = "requires Anthropic API key"]
async fn anthropic_complete_request() {
    use rskit_llm::LlmProvider;

    let cfg: AnthropicConfig = serde_json::from_str(r#"{"api_key":"sk-ant-real-key"}"#).unwrap();
    let provider = rskit_anthropic::AnthropicProvider::new(cfg).unwrap();
    let req = CompletionRequest {
        model: "claude-3-haiku-20240307".into(),
        messages: vec![user("Say hello.")],
        max_tokens: Some(10),
        temperature: Some(0.0),
        stream: false,
        tools: None,
        tool_choice: None,
    };
    let resp = provider.complete(req).await.unwrap();
    assert!(!resp.text().is_empty());
}
