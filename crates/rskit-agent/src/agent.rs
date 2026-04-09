//! Agent — the multi-turn agentic execution loop.

use std::pin::Pin;
use std::sync::Arc;

use async_stream::stream;
use futures::Stream;
use rskit_errors::AppResult;
use rskit_hook::{Action, HookRegistry};
use rskit_llm::provider::{Provider, count_tokens_approx};
use rskit_llm::types::{AssistantMessage, CompletionRequest, CompletionResponse, Message, Usage};
use rskit_tool::{Context, Registry, ToolResult};

use crate::hooks;
use crate::types::{AgentEvent, AgentResult, ContextStrategy, FailStrategy, StopReason};

// ── AgentConfig ─────────────────────────────────────────────────────────────

/// Configuration for an [`Agent`].
pub struct AgentConfig {
    /// The LLM provider to use for completions.
    pub provider: Arc<dyn Provider>,
    /// Optional tool registry.
    pub tools: Option<Arc<Registry>>,
    /// Optional hook registry for lifecycle events.
    pub hooks: Option<Arc<HookRegistry>>,
    /// System prompt prepended to every completion request.
    pub system_prompt: String,
    /// Maximum number of turns before the agent stops.
    pub max_turns: u32,
    /// Maximum cumulative token budget (input + output) across all turns.
    pub max_token_budget: usize,
    /// Strategy for compacting context when it exceeds the provider's limit.
    pub context_strategy: Option<Box<dyn ContextStrategy>>,
}

// ── Agent ───────────────────────────────────────────────────────────────────

/// A multi-turn agentic loop that drives an LLM provider, executes tool calls,
/// and emits hook events at each lifecycle point.
pub struct Agent {
    config: AgentConfig,
}

impl Agent {
    /// Create a new agent with the given configuration.
    pub fn new(config: AgentConfig) -> Self {
        Self { config }
    }

    /// Run the agent loop synchronously (all turns, no streaming).
    pub async fn run(&self, messages: Vec<Message>) -> AppResult<AgentResult> {
        let mut all_messages = vec![rskit_llm::system(&self.config.system_prompt)];
        all_messages.extend(messages);

        let mut total_usage = Usage {
            input_tokens: 0,
            output_tokens: 0,
        };
        let mut last_assistant = AssistantMessage {
            content: vec![],
            tool_calls: vec![],
            usage: None,
        };

        for turn in 0..self.config.max_turns {
            // ── TurnStart hook ──────────────────────────────────────────
            if let Some(ref hooks) = self.config.hooks {
                let result = hooks.emit(&hooks::TurnStart { turn });
                if result.action == Action::Abort {
                    return Ok(AgentResult {
                        messages: all_messages,
                        final_message: last_assistant,
                        total_usage,
                        turn_count: turn,
                        stop_reason: StopReason::Aborted,
                    });
                }
            }

            // ── Build request ───────────────────────────────────────────
            let tool_defs = self.config.tools.as_ref().map(|t| t.list());
            let mut request = CompletionRequest {
                model: self.config.provider.capabilities().model_id.clone(),
                messages: all_messages.clone(),
                max_tokens: None,
                temperature: None,
                stream: false,
                tools: tool_defs,
                tool_choice: None,
            };

            // ── PreLLMCall hook (allow Modify) ──────────────────────────
            if let Some(ref hooks) = self.config.hooks {
                let result = hooks.emit(&hooks::PreLLMCall {
                    request: request.clone(),
                });
                match result.action {
                    Action::Abort => {
                        return Ok(AgentResult {
                            messages: all_messages,
                            final_message: last_assistant,
                            total_usage,
                            turn_count: turn,
                            stop_reason: StopReason::Aborted,
                        });
                    }
                    Action::Modify => {
                        if let Some(data) = result.modified_data {
                            if let Some(modified) = data.downcast_ref::<CompletionRequest>() {
                                request = modified.clone();
                            }
                        }
                    }
                    Action::Continue => {}
                }
            }

            // ── LLM call ────────────────────────────────────────────────
            let response: CompletionResponse = self.config.provider.complete(request).await?;

            // ── PostLLMCall hook ─────────────────────────────────────────
            if let Some(ref hooks) = self.config.hooks {
                let result = hooks.emit(&hooks::PostLLMCall {
                    response: response.clone(),
                    error: None,
                });
                if result.action == Action::Abort {
                    return Ok(AgentResult {
                        messages: all_messages,
                        final_message: response.message,
                        total_usage,
                        turn_count: turn + 1,
                        stop_reason: StopReason::Aborted,
                    });
                }
            }

            // Track usage
            total_usage.input_tokens += response.usage.input_tokens;
            total_usage.output_tokens += response.usage.output_tokens;

            last_assistant = response.message.clone();
            all_messages.push(Message::Assistant(response.message.clone()));

            // ── Check for tool calls ────────────────────────────────────
            if !response.has_tool_calls() {
                return Ok(AgentResult {
                    messages: all_messages,
                    final_message: last_assistant,
                    total_usage,
                    turn_count: turn + 1,
                    stop_reason: StopReason::EndTurn,
                });
            }

            // ── Execute tool calls ──────────────────────────────────────
            if let Some(ref tools) = self.config.tools {
                for tc in &response.message.tool_calls {
                    let input: serde_json::Value = serde_json::from_str(&tc.function.arguments)
                        .unwrap_or(serde_json::Value::Null);

                    // PreToolCall hook
                    if let Some(ref hooks) = self.config.hooks {
                        let result = hooks.emit(&hooks::PreToolCall {
                            name: tc.function.name.clone(),
                            input: input.clone(),
                        });
                        if result.action == Action::Abort {
                            return Ok(AgentResult {
                                messages: all_messages,
                                final_message: last_assistant,
                                total_usage,
                                turn_count: turn + 1,
                                stop_reason: StopReason::Aborted,
                            });
                        }
                    }

                    let ctx = Context::new();
                    let tool_result = tools.call(&tc.function.name, &ctx, input.clone()).await;

                    let (result_opt, error_opt): (Option<ToolResult>, Option<String>) =
                        match &tool_result {
                            Ok(r) => (Some(r.clone()), None),
                            Err(e) => (None, Some(e.to_string())),
                        };

                    // PostToolCall hook
                    if let Some(ref hooks) = self.config.hooks {
                        hooks.emit(&hooks::PostToolCall {
                            name: tc.function.name.clone(),
                            input: input.clone(),
                            result: result_opt.clone(),
                            error: error_opt.clone(),
                        });
                    }

                    // Build tool result message
                    let (content, is_error) = match tool_result {
                        Ok(r) => (r.content, r.is_error),
                        Err(e) => (e.to_string(), true),
                    };

                    all_messages.push(rskit_llm::tool_result_msg(&tc.id, &content, is_error));
                }
            }

            // ── Check token budget ──────────────────────────────────────
            let total_tokens = (total_usage.input_tokens + total_usage.output_tokens) as usize;
            if total_tokens >= self.config.max_token_budget {
                return Ok(AgentResult {
                    messages: all_messages,
                    final_message: last_assistant,
                    total_usage,
                    turn_count: turn + 1,
                    stop_reason: StopReason::MaxBudget,
                });
            }

            // ── Check context size, compact if needed ───────────────────
            let caps = self.config.provider.capabilities();
            let context_tokens = count_tokens_approx(&all_messages);
            if context_tokens > caps.max_context_tokens && caps.max_context_tokens > 0 {
                let strategy = self
                    .config
                    .context_strategy
                    .as_ref()
                    .map(|s| s.as_ref())
                    .unwrap_or(&FailStrategy as &dyn ContextStrategy);

                all_messages = strategy.compact(all_messages, caps.max_context_tokens)?;
            }

            // ── TurnEnd hook ────────────────────────────────────────────
            if let Some(ref hooks) = self.config.hooks {
                let result = hooks.emit(&hooks::TurnEnd {
                    turn,
                    message: last_assistant.clone(),
                });
                if result.action == Action::Abort {
                    return Ok(AgentResult {
                        messages: all_messages,
                        final_message: last_assistant,
                        total_usage,
                        turn_count: turn + 1,
                        stop_reason: StopReason::Aborted,
                    });
                }
            }
        }

        // Exhausted max_turns
        Ok(AgentResult {
            messages: all_messages,
            final_message: last_assistant,
            total_usage,
            turn_count: self.config.max_turns,
            stop_reason: StopReason::MaxTurns,
        })
    }

    /// Stream the agent loop, yielding [`AgentEvent`]s for each lifecycle point.
    pub fn stream(
        &self,
        messages: Vec<Message>,
    ) -> Pin<Box<dyn Stream<Item = AgentEvent> + Send + '_>> {
        Box::pin(stream! {
            match self.run(messages).await {
                Ok(result) => {
                    for turn in 0..result.turn_count {
                        yield AgentEvent::TurnStart { turn };
                        yield AgentEvent::TurnComplete {
                            turn,
                            message: result.final_message.clone(),
                            usage: result.total_usage.clone(),
                        };
                    }
                    yield AgentEvent::Complete { result };
                }
                Err(e) => {
                    tracing::error!(error = %e, "agent.run.failed");
                }
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use futures::StreamExt;
    use rskit_errors::AppError;
    use rskit_hook::HookResult;
    use rskit_llm::provider::Capabilities;
    use rskit_llm::stream_events::StreamEvent;
    use rskit_llm::types;
    use std::sync::atomic::{AtomicU32, Ordering};

    // ── Mock provider ───────────────────────────────────────────────────

    struct MockProvider {
        responses: Vec<CompletionResponse>,
        call_count: AtomicU32,
    }

    impl MockProvider {
        fn new(responses: Vec<CompletionResponse>) -> Self {
            Self {
                responses,
                call_count: AtomicU32::new(0),
            }
        }

        fn single_text(text: &str) -> Self {
            Self::new(vec![CompletionResponse {
                message: AssistantMessage {
                    content: types::text_content(text),
                    tool_calls: vec![],
                    usage: None,
                },
                model: "mock".to_string(),
                usage: Usage {
                    input_tokens: 10,
                    output_tokens: 5,
                },
                stop_reason: Some(rskit_llm::StopReason::EndTurn),
            }])
        }
    }

    #[async_trait]
    impl Provider for MockProvider {
        async fn complete(
            &self,
            _request: CompletionRequest,
        ) -> Result<CompletionResponse, AppError> {
            let idx = self.call_count.fetch_add(1, Ordering::SeqCst) as usize;
            if idx < self.responses.len() {
                Ok(self.responses[idx].clone())
            } else {
                // Return last response for any additional calls
                Ok(self.responses.last().unwrap().clone())
            }
        }

        async fn stream(
            &self,
            _request: CompletionRequest,
        ) -> Result<Pin<Box<dyn Stream<Item = StreamEvent> + Send>>, AppError> {
            Ok(Box::pin(futures::stream::empty()))
        }

        fn capabilities(&self) -> Capabilities {
            Capabilities {
                supports_tools: true,
                supports_streaming: false,
                max_context_tokens: 128_000,
                max_output_tokens: 4_096,
                model_id: "mock".to_string(),
                ..Default::default()
            }
        }

        fn count_tokens(&self, messages: &[Message]) -> usize {
            count_tokens_approx(messages)
        }
    }

    // ── Tests ───────────────────────────────────────────────────────────

    #[tokio::test]
    async fn test_agent_simple_completion() {
        let provider = Arc::new(MockProvider::single_text("Hello!"));
        let agent = Agent::new(AgentConfig {
            provider,
            tools: None,
            hooks: None,
            system_prompt: "You are helpful.".to_string(),
            max_turns: 5,
            max_token_budget: 100_000,
            context_strategy: None,
        });

        let result = agent.run(vec![types::user("Hi")]).await.unwrap();
        assert_eq!(result.turn_count, 1);
        assert!(matches!(result.stop_reason, StopReason::EndTurn));
        assert_eq!(result.total_usage.input_tokens, 10);
        assert_eq!(result.total_usage.output_tokens, 5);
    }

    #[tokio::test]
    async fn test_agent_max_turns() {
        // Provider always returns tool calls → agent loops until max_turns
        let tool_call_response = CompletionResponse {
            message: AssistantMessage {
                content: vec![],
                tool_calls: vec![rskit_llm::ToolCall {
                    id: "tc_1".to_string(),
                    call_type: "function".to_string(),
                    function: rskit_llm::FunctionCall {
                        name: "test_tool".to_string(),
                        arguments: r#"{"x": 1}"#.to_string(),
                    },
                }],
                usage: None,
            },
            model: "mock".to_string(),
            usage: Usage {
                input_tokens: 5,
                output_tokens: 5,
            },
            stop_reason: Some(rskit_llm::StopReason::ToolUse),
        };

        let provider = Arc::new(MockProvider::new(vec![tool_call_response]));

        // No tools registered → tool calls will fail, but loop continues
        let agent = Agent::new(AgentConfig {
            provider,
            tools: None,
            hooks: None,
            system_prompt: "sys".to_string(),
            max_turns: 3,
            max_token_budget: 100_000,
            context_strategy: None,
        });

        let result = agent.run(vec![types::user("go")]).await.unwrap();
        assert_eq!(result.turn_count, 3);
        assert!(matches!(result.stop_reason, StopReason::MaxTurns));
    }

    #[tokio::test]
    async fn test_agent_with_tool() {
        use rskit_tool::{from_fn, text_result};
        use schemars::JsonSchema;
        use serde::Deserialize;

        #[derive(Deserialize, JsonSchema)]
        struct AddInput {
            a: i32,
            b: i32,
        }

        let registry = Arc::new(Registry::new());
        registry
            .register(from_fn(
                "add",
                "Add two numbers",
                |_ctx: Context, input: AddInput| async move {
                    Ok(text_result(&format!("{}", input.a + input.b)))
                },
            ))
            .unwrap();

        // First call: model requests tool
        let tool_call_resp = CompletionResponse {
            message: AssistantMessage {
                content: vec![],
                tool_calls: vec![rskit_llm::ToolCall {
                    id: "tc_1".to_string(),
                    call_type: "function".to_string(),
                    function: rskit_llm::FunctionCall {
                        name: "add".to_string(),
                        arguments: r#"{"a": 2, "b": 3}"#.to_string(),
                    },
                }],
                usage: None,
            },
            model: "mock".to_string(),
            usage: Usage {
                input_tokens: 10,
                output_tokens: 5,
            },
            stop_reason: Some(rskit_llm::StopReason::ToolUse),
        };

        // Second call: model returns final text
        let final_resp = CompletionResponse {
            message: AssistantMessage {
                content: types::text_content("The answer is 5"),
                tool_calls: vec![],
                usage: None,
            },
            model: "mock".to_string(),
            usage: Usage {
                input_tokens: 15,
                output_tokens: 8,
            },
            stop_reason: Some(rskit_llm::StopReason::EndTurn),
        };

        let provider = Arc::new(MockProvider::new(vec![tool_call_resp, final_resp]));

        let agent = Agent::new(AgentConfig {
            provider,
            tools: Some(registry),
            hooks: None,
            system_prompt: "You are a calculator.".to_string(),
            max_turns: 5,
            max_token_budget: 100_000,
            context_strategy: None,
        });

        let result = agent.run(vec![types::user("What is 2+3?")]).await.unwrap();
        assert_eq!(result.turn_count, 2);
        assert!(matches!(result.stop_reason, StopReason::EndTurn));
        // Usage should be summed
        assert_eq!(result.total_usage.input_tokens, 25);
        assert_eq!(result.total_usage.output_tokens, 13);
    }

    #[tokio::test]
    async fn test_agent_max_budget() {
        let tool_call_response = CompletionResponse {
            message: AssistantMessage {
                content: vec![],
                tool_calls: vec![rskit_llm::ToolCall {
                    id: "tc_1".to_string(),
                    call_type: "function".to_string(),
                    function: rskit_llm::FunctionCall {
                        name: "noop".to_string(),
                        arguments: "{}".to_string(),
                    },
                }],
                usage: None,
            },
            model: "mock".to_string(),
            usage: Usage {
                input_tokens: 50,
                output_tokens: 50,
            },
            stop_reason: Some(rskit_llm::StopReason::ToolUse),
        };

        let provider = Arc::new(MockProvider::new(vec![tool_call_response]));

        let agent = Agent::new(AgentConfig {
            provider,
            tools: None,
            hooks: None,
            system_prompt: "sys".to_string(),
            max_turns: 100,
            max_token_budget: 80, // Budget of 80 tokens total
            context_strategy: None,
        });

        let result = agent.run(vec![types::user("go")]).await.unwrap();
        assert!(matches!(result.stop_reason, StopReason::MaxBudget));
    }

    #[tokio::test]
    async fn test_agent_hook_abort() {
        let provider = Arc::new(MockProvider::single_text("Hello"));
        let hooks = Arc::new(HookRegistry::new());

        let _unsub = hooks.on(crate::turn_start_type(), |_| {
            HookResult::abort("blocked by policy")
        });

        let agent = Agent::new(AgentConfig {
            provider,
            tools: None,
            hooks: Some(hooks),
            system_prompt: "sys".to_string(),
            max_turns: 5,
            max_token_budget: 100_000,
            context_strategy: None,
        });

        let result = agent.run(vec![types::user("hi")]).await.unwrap();
        assert!(matches!(result.stop_reason, StopReason::Aborted));
        assert_eq!(result.turn_count, 0);
    }

    #[tokio::test]
    async fn test_agent_hook_counts() {
        let provider = Arc::new(MockProvider::single_text("done"));
        let hooks = Arc::new(HookRegistry::new());

        let pre_count = Arc::new(AtomicU32::new(0));
        let post_count = Arc::new(AtomicU32::new(0));

        let pc = pre_count.clone();
        let _unsub1 = hooks.on(crate::pre_llm_call_type(), move |_| {
            pc.fetch_add(1, Ordering::SeqCst);
            HookResult::ok()
        });

        let poc = post_count.clone();
        let _unsub2 = hooks.on(crate::post_llm_call_type(), move |_| {
            poc.fetch_add(1, Ordering::SeqCst);
            HookResult::ok()
        });

        let agent = Agent::new(AgentConfig {
            provider,
            tools: None,
            hooks: Some(hooks),
            system_prompt: "sys".to_string(),
            max_turns: 5,
            max_token_budget: 100_000,
            context_strategy: None,
        });

        agent.run(vec![types::user("hi")]).await.unwrap();
        assert_eq!(pre_count.load(Ordering::SeqCst), 1);
        assert_eq!(post_count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_agent_stream() {
        let provider = Arc::new(MockProvider::single_text("streamed"));
        let agent = Agent::new(AgentConfig {
            provider,
            tools: None,
            hooks: None,
            system_prompt: "sys".to_string(),
            max_turns: 5,
            max_token_budget: 100_000,
            context_strategy: None,
        });

        let stream = agent.stream(vec![types::user("hi")]);
        let events: Vec<AgentEvent> = stream.collect().await;
        assert!(!events.is_empty());

        // Last event should be Complete
        let last = events.last().unwrap();
        assert!(matches!(last, AgentEvent::Complete { .. }));
    }
}
