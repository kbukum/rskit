use rskit_llm::{CompletionRequest, Message, assistant, system, user};

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
