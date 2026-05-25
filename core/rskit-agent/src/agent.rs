//! Agent — the multi-turn agentic execution loop.

use std::pin::Pin;
use std::sync::Arc;

use async_stream::stream;
use futures::Stream;
use rskit_ai::semconv;
use rskit_component::{Component, Health};
use rskit_errors::AppResult;
use rskit_hook::{CancellationToken, Event, HookRegistry};
use rskit_llm::provider::Provider;
use rskit_llm::types::{CompletionRequest, CompletionResponse, Message, ToolDefinition};
use rskit_tool::{Registry, ToolInput, ToolResult};
use tracing::Instrument;

use crate::config::AgentConfig;
use crate::hooks;
use crate::runner::RunState;
use crate::stop;
use crate::tool_exec;
use crate::types::{AgentEvent, AgentResult, StopReason};

// ── Agent ───────────────────────────────────────────────────────────────────

/// A multi-turn agentic loop that drives an LLM provider, executes tool calls,
/// and emits hook events at each lifecycle point.
pub struct Agent {
    provider: Arc<dyn Provider>,
    config: AgentConfig,
}

fn emit_hook<E: Event>(hooks: &HookRegistry, event: &E, token: CancellationToken) -> bool {
    match hooks.emit(event, token.clone()) {
        Ok(()) => false,
        Err(error) => {
            let fatal = error.is_fatal();
            let _ = hooks.emit(
                &hooks::OnError {
                    error: error.to_string(),
                    source: event.event_type().to_string(),
                },
                token,
            );
            fatal
        }
    }
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

    /// Run the agent loop synchronously (all turns, no streaming).
    pub async fn run(&self, messages: Vec<Message>) -> AppResult<AgentResult> {
        let run_span = tracing::info_span!(
            "agent.run",
            "gen_ai.system" = "agent",
            "gen_ai.operation.name" = semconv::Operation::AgentRun.as_str(),
            "gen_ai.request.model" = %self.config.model,
        );
        async move {
            let mut state = RunState::new(&self.config.system_prompt, messages);

            if let Some(stop_reason) = stop::initial_stop(&self.config) {
                return Ok(state.finish(0, stop_reason));
            }

            let hook_token = CancellationToken::new();

            for turn in 0..self.config.max_turns {
                let turn_span = tracing::info_span!(
                    "agent.turn",
                    "gen_ai.operation.name" = semconv::Operation::AgentTurn.as_str(),
                    "agent.turn" = turn,
                );

                if let Some(stop_reason) = stop::wall_clock_stop(&state, &self.config) {
                    return Ok(state.finish(turn, stop_reason));
                }
                if let Some(ref hooks) = self.config.hooks
                    && emit_hook(hooks, &hooks::TurnStart { turn }, hook_token.clone())
                {
                    return Ok(state.finish(turn, StopReason::Aborted));
                }

                let tool_defs = self.config.tools.as_ref().map(|registry| {
                    registry
                        .list()
                        .into_iter()
                        .map(|tool| ToolDefinition {
                            name: tool.name,
                            description: tool.description,
                            input_schema: tool.input_schema,
                            output_schema: tool.output_schema,
                        })
                        .collect()
                });
                let request = CompletionRequest {
                    model: self.config.model.clone(),
                    messages: state.messages.clone(),
                    max_tokens: None,
                    temperature: None,
                    stream: false,
                    tools: tool_defs,
                    tool_choice: None,
                };

                if let Some(ref hooks) = self.config.hooks
                    && emit_hook(
                        hooks,
                        &hooks::PreLLMCall {
                            request: request.clone(),
                        },
                        hook_token.clone(),
                    )
                {
                    return Ok(state.finish(turn, StopReason::Aborted));
                }

                let response: CompletionResponse = tokio::time::timeout(
                    state.remaining_wall_clock(self.config.wall_clock),
                    self.provider.complete(request),
                )
                .instrument(turn_span.clone())
                .await
                .map_err(|_| stop::wall_clock_error())??;

                if let Some(ref hooks) = self.config.hooks
                    && emit_hook(
                        hooks,
                        &hooks::PostLLMCall {
                            response: response.clone(),
                            error: None,
                        },
                        hook_token.clone(),
                    )
                {
                    state.last_assistant = response.message;
                    return Ok(state.finish(turn + 1, StopReason::Aborted));
                }

                let response_stop_reason = response
                    .stop_reason
                    .unwrap_or(rskit_llm::FinishReason::Stop);
                let has_tool_calls = response.has_tool_calls();
                state.record_response(response);

                if let Some(stop_reason) = stop::token_budget_stop(&state, &self.config) {
                    return Ok(state.finish(turn + 1, stop_reason));
                }

                if !has_tool_calls {
                    return Ok(state.finish(turn + 1, StopReason::from(response_stop_reason)));
                }

                if let Some(ref tools) = self.config.tools {
                    let tool_calls = state.last_assistant.tool_calls.clone();
                    for tc in &tool_calls {
                        if let Some(stop_reason) = stop::tool_budget_stop(&state, &self.config) {
                            return Ok(state.finish(turn + 1, stop_reason));
                        }
                        state.tool_calls_used += 1;
                        let input = ToolInput::new(serde_json::Value::Object(tc.input.clone()))?;

                        if let Some(ref hooks) = self.config.hooks
                            && emit_hook(
                                hooks,
                                &hooks::PreToolCall {
                                    name: tc.name.clone(),
                                    input: input.clone(),
                                },
                                hook_token.clone(),
                            )
                        {
                            return Ok(state.finish(turn + 1, StopReason::Aborted));
                        }

                        let tool_result = self
                            .execute_tool_call(tools, &tc.id, &tc.name, input.clone())
                            .instrument(turn_span.clone())
                            .await;

                        let (result_opt, error_opt): (Option<ToolResult>, Option<String>) =
                            match &tool_result {
                                Ok(r) => (Some(r.clone()), None),
                                Err(e) => (None, Some(e.to_string())),
                            };

                        if let Some(ref hooks) = self.config.hooks {
                            let _ = emit_hook(
                                hooks,
                                &hooks::PostToolCall {
                                    name: tc.name.clone(),
                                    input: input.clone(),
                                    result: result_opt.clone(),
                                    error: error_opt.clone(),
                                },
                                hook_token.clone(),
                            );
                        }

                        let (content, is_error) = match tool_result {
                            Ok(r) => (r.content, r.is_error),
                            Err(e) => (e.to_string(), true),
                        };

                        state
                            .messages
                            .push(rskit_llm::tool_result_msg(&tc.id, &content, is_error));
                    }
                }

                let caps = self.provider.capabilities();
                state.compact_context(
                    caps.max_input_tokens,
                    self.config.context_strategy.as_deref(),
                )?;

                if let Some(ref hooks) = self.config.hooks
                    && emit_hook(
                        hooks,
                        &hooks::TurnEnd {
                            turn,
                            message: state.last_assistant.clone(),
                        },
                        hook_token.clone(),
                    )
                {
                    return Ok(state.finish(turn + 1, StopReason::Aborted));
                }
            }

            // Exhausted max_turns
            Ok(state.finish(self.config.max_turns, StopReason::MaxTurns))
        }
        .instrument(run_span)
        .await
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
                            usage: result.total_usage,
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

    async fn execute_tool_call(
        &self,
        tools: &Registry,
        tool_use_id: &str,
        name: &str,
        input: ToolInput,
    ) -> AppResult<ToolResult> {
        tool_exec::execute_tool_call(
            tools,
            self.config.policy.clone(),
            self.config.tool_timeout,
            tool_use_id,
            name,
            input,
        )
        .await
    }
}

#[async_trait::async_trait]
impl Component for Agent {
    fn name(&self) -> &str {
        "rskit-agent"
    }

    async fn start(&self) -> AppResult<()> {
        Ok(())
    }

    async fn stop(&self) -> AppResult<()> {
        Ok(())
    }

    fn health(&self) -> Health {
        Health::healthy(self.name())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use futures::StreamExt;
    use rskit_ai::Capabilities;
    use rskit_ai::StreamEventRef;
    use rskit_ai::chat::count_tokens_approx;
    use rskit_errors::AppError;
    use rskit_hook::HookError;
    use rskit_llm::types::{self, AssistantMessage, CompletionRequest, CompletionResponse, Usage};
    use rskit_resilience::{ConstantBackoff, Policy, RetryPolicy};
    use rskit_tool::Context;
    use std::sync::atomic::{AtomicU32, Ordering};
    use std::time::Duration;

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
        assert!(!events.is_empty());

        // Last event should be Complete
        let last = events.last().unwrap();
        assert!(matches!(last, AgentEvent::Complete { .. }));
    }
}
