//! Agent — the multi-turn agentic execution loop.

use std::sync::Arc;

use rskit_llm::provider::Provider;

use crate::config::AgentConfig;

mod component;
mod run;
mod stream;

/// A multi-turn agentic loop that drives an LLM provider, executes tool calls,
/// and emits hook events at each lifecycle point.
pub struct Agent {
    provider: Arc<dyn Provider>,
    config: AgentConfig,
}

impl Agent {
    /// Create a new agent with the given provider and configuration.
    pub fn new(provider: Arc<dyn Provider>, config: AgentConfig) -> Self {
        Self { provider, config }
    }

    /// Create a new agent with the locked default configuration.
    pub fn with_defaults(provider: Arc<dyn Provider>) -> Self {
        Self::new(provider, AgentConfig::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use std::pin::Pin;

    use futures::{Stream, StreamExt};
    use rskit_ai::Capabilities;
    use rskit_ai::StreamEventRef;
    use rskit_ai::chat::count_tokens_approx;
    use rskit_errors::AppError;
    use rskit_hook::{HookError, HookRegistry};
    use rskit_llm::types::{
        self, AssistantMessage, CompletionRequest, CompletionResponse, Message, Usage,
    };
    use rskit_resilience::{ConstantBackoff, Policy, RetryPolicy};
    use rskit_tool::{Context, Registry};
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::time::Duration;

    use crate::types::{AgentEvent, StopReason};

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
                    cached_tokens: 0,
                    reasoning_tokens: 0,
                },
                stop_reason: Some(rskit_llm::FinishReason::Stop),
            }])
        }
    }

    #[async_trait]
    impl rskit_provider::Provider for MockProvider {
        fn name(&self) -> &'static str {
            "mock"
        }
    }

    #[async_trait]
    impl rskit_provider::RequestResponse<CompletionRequest, CompletionResponse> for MockProvider {
        async fn execute(&self, input: CompletionRequest) -> Result<CompletionResponse, AppError> {
            self.complete(input).await
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
        ) -> Result<Pin<Box<dyn Stream<Item = StreamEventRef> + Send>>, AppError> {
            Ok(Box::pin(futures::stream::empty()))
        }

        fn capabilities(&self) -> Capabilities {
            Capabilities {
                tool_use: true,
                streaming: false,
                max_input_tokens: Some(128_000),
                max_output_tokens: Some(4_096),
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
        let agent = Agent::new(
            provider,
            AgentConfig {
                tools: None,
                hooks: None,
                system_prompt: "You are helpful.".to_string(),
                max_turns: 5,
                max_tokens: 100_000,
                wall_clock: Duration::from_secs(60),
                max_tool_calls: 50,
                tool_concurrency: 4,
                tool_timeout: Duration::from_secs(30),
                policy: None,
                context_strategy: None,
                model: String::new(),
            },
        );

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
                tool_calls: vec![rskit_llm::ToolUseBlock {
                    id: "tc_1".to_string(),
                    name: "test_tool".to_string(),
                    input: serde_json::json!({"x": 1}).as_object().cloned().unwrap(),
                }],
                usage: None,
            },
            model: "mock".to_string(),
            usage: Usage {
                input_tokens: 5,
                output_tokens: 5,
                cached_tokens: 0,
                reasoning_tokens: 0,
            },
            stop_reason: Some(rskit_llm::FinishReason::ToolUse),
        };

        let provider = Arc::new(MockProvider::new(vec![tool_call_response]));

        // No tools registered → tool calls will fail, but loop continues
        let agent = Agent::new(
            provider,
            AgentConfig {
                tools: None,
                hooks: None,
                system_prompt: "sys".to_string(),
                max_turns: 3,
                max_tokens: 100_000,
                wall_clock: Duration::from_secs(60),
                max_tool_calls: 50,
                tool_concurrency: 4,
                tool_timeout: Duration::from_secs(30),
                policy: None,
                context_strategy: None,
                model: String::new(),
            },
        );

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
        let attempts = Arc::new(AtomicU32::new(0));
        let attempts_for_tool = Arc::clone(&attempts);
        registry
            .register(
                from_fn(
                    "add",
                    "Add two numbers",
                    move |_ctx: Context, input: AddInput| {
                        let attempts = Arc::clone(&attempts_for_tool);
                        async move {
                            let attempt = attempts.fetch_add(1, Ordering::SeqCst);
                            if attempt == 0 {
                                Err(AppError::connection_failed("add"))
                            } else {
                                Ok(text_result(&format!("{}", input.a + input.b)))
                            }
                        }
                    },
                )
                .unwrap(),
            )
            .unwrap();

        // First call: model requests tool
        let tool_call_resp = CompletionResponse {
            message: AssistantMessage {
                content: vec![],
                tool_calls: vec![rskit_llm::ToolUseBlock {
                    id: "tc_1".to_string(),
                    name: "add".to_string(),
                    input: serde_json::json!({"a": 2, "b": 3})
                        .as_object()
                        .cloned()
                        .unwrap(),
                }],
                usage: None,
            },
            model: "mock".to_string(),
            usage: Usage {
                input_tokens: 10,
                output_tokens: 5,
                cached_tokens: 0,
                reasoning_tokens: 0,
            },
            stop_reason: Some(rskit_llm::FinishReason::ToolUse),
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
                cached_tokens: 0,
                reasoning_tokens: 0,
            },
            stop_reason: Some(rskit_llm::FinishReason::Stop),
        };

        let provider = Arc::new(MockProvider::new(vec![tool_call_resp, final_resp]));

        let agent = Agent::new(
            provider,
            AgentConfig {
                tools: Some(registry),
                hooks: None,
                system_prompt: "You are a calculator.".to_string(),
                max_turns: 5,
                max_tokens: 100_000,
                wall_clock: Duration::from_secs(60),
                max_tool_calls: 50,
                tool_concurrency: 4,
                tool_timeout: Duration::from_secs(30),
                policy: Some(
                    Policy::new().with_retry(
                        RetryPolicy::new()
                            .with_max_attempts(2)
                            .with_constant_backoff(ConstantBackoff::new(Duration::from_millis(1)))
                            .with_jitter(false),
                    ),
                ),
                context_strategy: None,
                model: String::new(),
            },
        );

        let result = agent.run(vec![types::user("What is 2+3?")]).await.unwrap();
        assert_eq!(attempts.load(Ordering::SeqCst), 2);
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
                tool_calls: vec![rskit_llm::ToolUseBlock {
                    id: "tc_1".to_string(),
                    name: "noop".to_string(),
                    input: serde_json::Map::new(),
                }],
                usage: None,
            },
            model: "mock".to_string(),
            usage: Usage {
                input_tokens: 50,
                output_tokens: 50,
                cached_tokens: 0,
                reasoning_tokens: 0,
            },
            stop_reason: Some(rskit_llm::FinishReason::ToolUse),
        };

        let provider = Arc::new(MockProvider::new(vec![tool_call_response]));

        let agent = Agent::new(
            provider,
            AgentConfig {
                tools: None,
                hooks: None,
                system_prompt: "sys".to_string(),
                max_turns: 100,
                max_tokens: 80, // Budget of 80 tokens total
                wall_clock: Duration::from_secs(60),
                max_tool_calls: 50,
                tool_concurrency: 4,
                tool_timeout: Duration::from_secs(30),
                policy: None,
                context_strategy: None,
                model: String::new(),
            },
        );

        let result = agent.run(vec![types::user("go")]).await.unwrap();
        assert!(matches!(result.stop_reason, StopReason::MaxTokens));
    }

    #[tokio::test]
    async fn max_budget_stops_after_final_response_without_tool_calls() {
        let provider = Arc::new(MockProvider::new(vec![CompletionResponse {
            message: AssistantMessage {
                content: types::text_content("large final response"),
                tool_calls: vec![],
                usage: None,
            },
            model: "mock".to_string(),
            usage: Usage {
                input_tokens: 50,
                output_tokens: 50,
                cached_tokens: 0,
                reasoning_tokens: 0,
            },
            stop_reason: Some(rskit_llm::FinishReason::Stop),
        }]));

        let agent = Agent::new(
            provider,
            AgentConfig {
                tools: None,
                hooks: None,
                system_prompt: "sys".to_string(),
                max_turns: 5,
                max_tokens: 80,
                wall_clock: Duration::from_secs(60),
                max_tool_calls: 50,
                tool_concurrency: 4,
                tool_timeout: Duration::from_secs(30),
                policy: None,
                context_strategy: None,
                model: String::new(),
            },
        );

        let result = agent.run(vec![types::user("go")]).await.unwrap();
        assert!(matches!(result.stop_reason, StopReason::MaxTokens));
        assert_eq!(result.turn_count, 1);
    }

    #[tokio::test]
    async fn test_agent_hook_fatal_error_stops() {
        let provider = Arc::new(MockProvider::single_text("Hello"));
        let hooks = Arc::new(HookRegistry::new());

        let _unsub = hooks.on::<crate::hooks::TurnStart>(crate::turn_start_type(), |_, _| {
            Err(HookError::fatal("blocked by policy"))
        });

        let agent = Agent::new(
            provider,
            AgentConfig {
                tools: None,
                hooks: Some(hooks),
                system_prompt: "sys".to_string(),
                max_turns: 5,
                max_tokens: 100_000,
                wall_clock: Duration::from_secs(60),
                max_tool_calls: 50,
                tool_concurrency: 4,
                tool_timeout: Duration::from_secs(30),
                policy: None,
                context_strategy: None,
                model: String::new(),
            },
        );

        let result = agent.run(vec![types::user("hi")]).await.unwrap();
        assert!(matches!(result.stop_reason, StopReason::Aborted));
        assert_eq!(result.turn_count, 0);
    }

    #[tokio::test]
    async fn hook_observes_request_without_mutation_surface() {
        let provider = Arc::new(MockProvider::single_text("done"));
        let hooks = Arc::new(HookRegistry::new());
        let observed = Arc::new(AtomicU32::new(0));
        let observed_clone = Arc::clone(&observed);

        let _unsub =
            hooks.on::<crate::hooks::PreLLMCall>(crate::pre_llm_call_type(), move |_, event| {
                assert_eq!(event.request.model, "test-model");
                observed_clone.fetch_add(1, Ordering::SeqCst);
                Ok(())
            });

        let agent = Agent::new(
            provider,
            AgentConfig {
                hooks: Some(hooks),
                system_prompt: "sys".to_string(),
                model: "test-model".to_string(),
                ..AgentConfig::default()
            },
        );

        let result = agent.run(vec![types::user("hi")]).await.unwrap();
        assert!(matches!(result.stop_reason, StopReason::EndTurn));
        assert_eq!(observed.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_agent_hook_counts() {
        let provider = Arc::new(MockProvider::single_text("done"));
        let hooks = Arc::new(HookRegistry::new());

        let pre_count = Arc::new(AtomicU32::new(0));
        let post_count = Arc::new(AtomicU32::new(0));

        let pc = pre_count.clone();
        let _unsub1 =
            hooks.on::<crate::hooks::PreLLMCall>(crate::pre_llm_call_type(), move |_, _| {
                pc.fetch_add(1, Ordering::SeqCst);
                Ok(())
            });

        let poc = post_count.clone();
        let _unsub2 =
            hooks.on::<crate::hooks::PostLLMCall>(crate::post_llm_call_type(), move |_, _| {
                poc.fetch_add(1, Ordering::SeqCst);
                Ok(())
            });

        let agent = Agent::new(
            provider,
            AgentConfig {
                tools: None,
                hooks: Some(hooks),
                system_prompt: "sys".to_string(),
                max_turns: 5,
                max_tokens: 100_000,
                wall_clock: Duration::from_secs(60),
                max_tool_calls: 50,
                tool_concurrency: 4,
                tool_timeout: Duration::from_secs(30),
                policy: None,
                context_strategy: None,
                model: String::new(),
            },
        );

        agent.run(vec![types::user("hi")]).await.unwrap();
        assert_eq!(pre_count.load(Ordering::SeqCst), 1);
        assert_eq!(post_count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn test_agent_stream() {
        let provider = Arc::new(MockProvider::single_text("streamed"));
        let agent = Agent::new(
            provider,
            AgentConfig {
                tools: None,
                hooks: None,
                system_prompt: "sys".to_string(),
                max_turns: 5,
                max_tokens: 100_000,
                wall_clock: Duration::from_secs(60),
                max_tool_calls: 50,
                tool_concurrency: 4,
                tool_timeout: Duration::from_secs(30),
                policy: None,
                context_strategy: None,
                model: String::new(),
            },
        );

        let stream = agent.stream(vec![types::user("hi")]);
        let events: Vec<AgentEvent> = stream.collect().await;
        assert_eq!(events.len(), 3);
        assert!(matches!(
            events.first(),
            Some(AgentEvent::TurnStart { turn: 0 })
        ));
        assert!(matches!(
            events.get(1),
            Some(AgentEvent::TurnComplete {
                turn: 0,
                usage: Usage {
                    input_tokens: 10,
                    output_tokens: 5,
                    cached_tokens: 0,
                    reasoning_tokens: 0,
                },
                ..
            })
        ));
        assert!(matches!(events.last(), Some(AgentEvent::Complete { .. })));
    }
}
