//! Hook event types, actions, and handler definitions.

use rskit_llm::types::{AssistantMessage, CompletionRequest, CompletionResponse};
use rskit_tool::ToolResult;
use serde::{Deserialize, Serialize};

// ── EventType ───────────────────────────────────────────────────────────────

/// Discriminator for the kind of event being emitted.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventType {
    /// Before a tool is invoked.
    PreToolCall,
    /// After a tool completes (success or failure).
    PostToolCall,
    /// Before an LLM completion request is sent.
    #[serde(rename = "pre_llm_call")]
    PreLLMCall,
    /// After an LLM completion response is received.
    #[serde(rename = "post_llm_call")]
    PostLLMCall,
    /// When an error occurs in the pipeline.
    OnError,
    /// At the start of an agent turn.
    TurnStart,
    /// At the end of an agent turn.
    TurnEnd,
}

// ── HookEvent ───────────────────────────────────────────────────────────────

/// The concrete event payload delivered to hook handlers.
#[derive(Debug, Clone)]
pub enum HookEvent {
    /// Fired before a tool call is executed.
    PreToolCall {
        name: String,
        input: serde_json::Value,
    },
    /// Fired after a tool call completes.
    PostToolCall {
        name: String,
        input: serde_json::Value,
        result: Option<ToolResult>,
        error: Option<String>,
    },
    /// Fired before an LLM completion request is sent.
    PreLLMCall { request: CompletionRequest },
    /// Fired after an LLM completion response is received.
    PostLLMCall {
        response: CompletionResponse,
        error: Option<String>,
    },
    /// Fired when an error occurs anywhere in the pipeline.
    OnError { error: String, source: String },
    /// Fired at the start of an agent turn.
    TurnStart { turn: u32 },
    /// Fired at the end of an agent turn.
    TurnEnd {
        turn: u32,
        message: AssistantMessage,
    },
}

impl HookEvent {
    /// Return the [`EventType`] discriminator for this event.
    pub fn event_type(&self) -> EventType {
        match self {
            HookEvent::PreToolCall { .. } => EventType::PreToolCall,
            HookEvent::PostToolCall { .. } => EventType::PostToolCall,
            HookEvent::PreLLMCall { .. } => EventType::PreLLMCall,
            HookEvent::PostLLMCall { .. } => EventType::PostLLMCall,
            HookEvent::OnError { .. } => EventType::OnError,
            HookEvent::TurnStart { .. } => EventType::TurnStart,
            HookEvent::TurnEnd { .. } => EventType::TurnEnd,
        }
    }
}

// ── Action / HookResult ────────────────────────────────────────────────────

/// What the pipeline should do after processing a hook handler.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Continue normal execution.
    Continue,
    /// Abort the current operation.
    Abort,
    /// The handler has modified data (check `modified_data`).
    Modify,
}

/// The outcome returned by a hook handler.
#[derive(Debug, Clone)]
pub struct HookResult {
    /// The action the pipeline should take.
    pub action: Action,
    /// Optional modified payload (JSON-serialised) for `Action::Modify`.
    pub modified_data: Option<serde_json::Value>,
    /// Human-readable explanation.
    pub reason: String,
}

impl HookResult {
    /// Convenience: continue with no modifications.
    pub fn ok() -> Self {
        Self {
            action: Action::Continue,
            modified_data: None,
            reason: String::new(),
        }
    }

    /// Convenience: abort with a reason.
    pub fn abort(reason: impl Into<String>) -> Self {
        Self {
            action: Action::Abort,
            modified_data: None,
            reason: reason.into(),
        }
    }

    /// Convenience: modify with new data.
    pub fn modify(data: serde_json::Value, reason: impl Into<String>) -> Self {
        Self {
            action: Action::Modify,
            modified_data: Some(data),
            reason: reason.into(),
        }
    }
}

impl Default for HookResult {
    fn default() -> Self {
        Self::ok()
    }
}

/// A boxed function that handles a hook event.
pub type HookHandler = Box<dyn Fn(&HookEvent) -> HookResult + Send + Sync>;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_type_serde() {
        let json = serde_json::to_string(&EventType::PreToolCall).unwrap();
        assert_eq!(json, r#""pre_tool_call""#);

        let deser: EventType = serde_json::from_str(r#""post_llm_call""#).unwrap();
        assert_eq!(deser, EventType::PostLLMCall);
    }

    #[test]
    fn test_event_type_all_variants() {
        let variants = [
            EventType::PreToolCall,
            EventType::PostToolCall,
            EventType::PreLLMCall,
            EventType::PostLLMCall,
            EventType::OnError,
            EventType::TurnStart,
            EventType::TurnEnd,
        ];
        for v in &variants {
            let json = serde_json::to_value(v).unwrap();
            let deser: EventType = serde_json::from_value(json).unwrap();
            assert_eq!(*v, deser);
        }
    }

    #[test]
    fn test_hook_event_type_mapping() {
        let pre_tool = HookEvent::PreToolCall {
            name: "test".to_string(),
            input: serde_json::json!({}),
        };
        assert_eq!(pre_tool.event_type(), EventType::PreToolCall);

        let on_error = HookEvent::OnError {
            error: "err".to_string(),
            source: "src".to_string(),
        };
        assert_eq!(on_error.event_type(), EventType::OnError);

        let turn_start = HookEvent::TurnStart { turn: 1 };
        assert_eq!(turn_start.event_type(), EventType::TurnStart);
    }

    #[test]
    fn test_hook_result_ok() {
        let r = HookResult::ok();
        assert_eq!(r.action, Action::Continue);
        assert!(r.modified_data.is_none());
        assert!(r.reason.is_empty());
    }

    #[test]
    fn test_hook_result_abort() {
        let r = HookResult::abort("safety check failed");
        assert_eq!(r.action, Action::Abort);
        assert_eq!(r.reason, "safety check failed");
    }

    #[test]
    fn test_hook_result_modify() {
        let data = serde_json::json!({"temperature": 0.5});
        let r = HookResult::modify(data.clone(), "lowered temperature");
        assert_eq!(r.action, Action::Modify);
        assert_eq!(r.modified_data.unwrap(), data);
    }

    #[test]
    fn test_hook_result_default() {
        let r = HookResult::default();
        assert_eq!(r.action, Action::Continue);
    }

    #[test]
    fn test_action_equality() {
        assert_eq!(Action::Continue, Action::Continue);
        assert_ne!(Action::Continue, Action::Abort);
        assert_ne!(Action::Abort, Action::Modify);
    }
}
