//! Agent — the multi-turn agentic execution loop.

use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use async_stream::stream;
use futures::Stream;
use rskit_ai::chat::count_tokens_approx;
use rskit_ai::semconv;
use rskit_component::{Component, Health};
use rskit_errors::AppResult;
use rskit_hook::{CancellationToken, Event, HookRegistry};
use rskit_llm::provider::Provider;
use rskit_llm::types::{
    AssistantMessage, CompletionRequest, CompletionResponse, Message, ToolDefinition, Usage,
};
use rskit_resilience::Policy;
use rskit_tool::{Context, Registry, ToolResult};
use tracing::Instrument;

use crate::hooks;
use crate::types::{AgentEvent, AgentResult, ContextStrategy, FailStrategy, StopReason};

// ── AgentConfig ─────────────────────────────────────────────────────────────

/// Configuration for an [`Agent`].
pub struct AgentConfig {
    /// Optional tool registry.
    pub tools: Option<Arc<Registry>>,
    /// Optional hook registry for lifecycle events.
    pub hooks: Option<Arc<HookRegistry>>,
    /// System prompt prepended to every completion request.
    pub system_prompt: String,
    /// Maximum number of turns before the agent stops.
    pub max_turns: u32,
    /// Maximum cumulative token budget (input + output) across all turns.
    pub max_tokens: usize,
    /// Maximum wall-clock time for a run.
    pub wall_clock: Duration,
    /// Maximum logical tool calls. Retries count as one logical call.
    pub max_tool_calls: u32,
    /// Maximum concurrently scheduled tool calls.
    pub tool_concurrency: usize,
    /// Per-tool call timeout.
    pub tool_timeout: Duration,
    /// Optional resilience policy applied to tool executions.
    pub policy: Option<Policy>,
    /// Strategy for compacting context when it exceeds the provider's limit.
    pub context_strategy: Option<Box<dyn ContextStrategy>>,
    /// Model identifier to send with each completion request.
    pub model: String,
}

impl AgentConfig {
    /// Return this configuration as shared GenAI budget vocabulary.
    #[must_use]
    pub fn budget(&self) -> rskit_ai::Budget {
        rskit_ai::Budget {
            max_tokens: Some(self.max_tokens as u64),
            max_calls: Some(u64::from(self.max_tool_calls)),
            max_cost: None,
            wall_clock: Some(self.wall_clock.as_secs()),
        }
    }
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            tools: None,
            hooks: None,
            system_prompt: String::new(),
            max_turns: 10,
            max_tokens: 100_000,
            wall_clock: Duration::from_secs(60),
            max_tool_calls: 50,
            tool_concurrency: 4,
            tool_timeout: Duration::from_secs(30),
            policy: None,
            context_strategy: None,
            model: String::new(),
        }
    }
}

// ── Agent ───────────────────────────────────────────────────────────────────

/// A multi-turn agentic loop that drives an LLM provider, executes tool calls,
/// and emits hook events at each lifecycle point.
pub struct Agent {
    provider: Arc<dyn Provider>,
    config: AgentConfig,
}

fn emit_hook(hooks: &HookRegistry, event: &dyn Event, token: CancellationToken) -> bool {
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
            let mut all_messages = vec![rskit_llm::system(&self.config.system_prompt)];
            all_messages.extend(messages);

            let mut total_usage = Usage {
                input_tokens: 0,
                output_tokens: 0,
                cached_tokens: 0,
                reasoning_tokens: 0,
            };
            let mut last_assistant = AssistantMessage {
                content: vec![],
                tool_calls: vec![],
                usage: None,
            };

            if self.config.max_tokens == 0 {
                return Ok(AgentResult {
                    messages: all_messages,
                    final_message: last_assistant,
                    total_usage,
                    turn_count: 0,
                    stop_reason: StopReason::MaxTokens,
                });
            }

            let started_at = tokio::time::Instant::now();
            let mut tool_calls_used = 0_u32;
            let hook_token = CancellationToken::new();

            for turn in 0..self.config.max_turns {
                let turn_span = tracing::info_span!(
                    "agent.turn",
                    "gen_ai.operation.name" = semconv::Operation::AgentTurn.as_str(),
                    "agent.turn" = turn,
                );

                if started_at.elapsed() >= self.config.wall_clock {
                    return Ok(AgentResult {
                        messages: all_messages,
                        final_message: last_assistant,
                        total_usage,
                        turn_count: turn,
                        stop_reason: StopReason::WallClockExceeded,
                    });
                }
                if let Some(ref hooks) = self.config.hooks
                    && emit_hook(hooks, &hooks::TurnStart { turn }, hook_token.clone())
                {
                    return Ok(AgentResult {
                        messages: all_messages,
                        final_message: last_assistant,
                        total_usage,
                        turn_count: turn,
                        stop_reason: StopReason::Aborted,
                    });
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
                    messages: all_messages.clone(),
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
                    return Ok(AgentResult {
                        messages: all_messages,
                        final_message: last_assistant,
                        total_usage,
                        turn_count: turn,
                        stop_reason: StopReason::Aborted,
                    });
                }

                let response: CompletionResponse = tokio::time::timeout(
                    self.config.wall_clock.saturating_sub(started_at.elapsed()),
                    self.provider.complete(request),
                )
                .instrument(turn_span.clone())
                .await
                .map_err(|_| {
                    rskit_errors::AppError::new(
                        rskit_errors::ErrorCode::Timeout,
                        "agent wall-clock budget exceeded",
                    )
                })??;

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
                    return Ok(AgentResult {
                        messages: all_messages,
                        final_message: response.message,
                        total_usage,
                        turn_count: turn + 1,
                        stop_reason: StopReason::Aborted,
                    });
                }

                total_usage.input_tokens += response.usage.input_tokens;
                total_usage.output_tokens += response.usage.output_tokens;

                last_assistant = response.message.clone();
                all_messages.push(Message::Assistant(response.message.clone()));

                if !response.has_tool_calls() {
                    return Ok(AgentResult {
                        messages: all_messages,
                        final_message: last_assistant,
                        total_usage,
                        turn_count: turn + 1,
                        stop_reason: StopReason::from(
                            response
                                .stop_reason
                                .unwrap_or(rskit_llm::FinishReason::Stop),
                        ),
                    });
                }

                if let Some(ref tools) = self.config.tools {
                    for tc in &response.message.tool_calls {
                        if tool_calls_used >= self.config.max_tool_calls {
                            return Ok(AgentResult {
                                messages: all_messages,
                                final_message: last_assistant,
                                total_usage,
                                turn_count: turn + 1,
                                stop_reason: StopReason::MaxToolCallsExceeded,
                            });
                        }
                        tool_calls_used += 1;
                        let input = serde_json::Value::Object(tc.input.clone());

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
                            return Ok(AgentResult {
                                messages: all_messages,
                                final_message: last_assistant,
                                total_usage,
                                turn_count: turn + 1,
                                stop_reason: StopReason::Aborted,
                            });
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

                        all_messages.push(rskit_llm::tool_result_msg(&tc.id, &content, is_error));
                    }
                }

                let total_tokens = (total_usage.input_tokens + total_usage.output_tokens) as usize;
                if total_tokens >= self.config.max_tokens {
                    return Ok(AgentResult {
                        messages: all_messages,
                        final_message: last_assistant,
                        total_usage,
                        turn_count: turn + 1,
                        stop_reason: StopReason::MaxTokens,
                    });
                }

                let caps = self.provider.capabilities();
                let context_tokens = count_tokens_approx(&all_messages);
                if let Some(max_input_tokens) = caps.max_input_tokens
                    && context_tokens > max_input_tokens as usize
                {
                    let strategy = self
                        .config
                        .context_strategy
                        .as_ref()
                        .map(|s| s.as_ref())
                        .unwrap_or(&FailStrategy as &dyn ContextStrategy);

                    all_messages = strategy.compact(all_messages, max_input_tokens as usize)?;
                }

                if let Some(ref hooks) = self.config.hooks
                    && emit_hook(
                        hooks,
                        &hooks::TurnEnd {
                            turn,
                            message: last_assistant.clone(),
                        },
                        hook_token.clone(),
                    )
                {
                    return Ok(AgentResult {
                        messages: all_messages,
                        final_message: last_assistant,
                        total_usage,
                        turn_count: turn + 1,
                        stop_reason: StopReason::Aborted,
                    });
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
        input: serde_json::Value,
    ) -> AppResult<ToolResult> {
        let timeout = self.config.tool_timeout;
        let policy = self.config.policy.clone();
        let tool_name = name.to_string();
        let tool_use_id = tool_use_id.to_string();

        let execute = || {
            let tool_name = tool_name.clone();
            let tool_use_id = tool_use_id.clone();
            let input = input.clone();
            async move {
                let mut ctx = Context::new();
                ctx.tool_use_id = tool_use_id;
                tokio::time::timeout(timeout, tools.call(&tool_name, &ctx, input))
                    .await
                    .unwrap_or_else(|_| {
                        Err(rskit_errors::AppError::new(
                            rskit_errors::ErrorCode::Timeout,
                            "tool call timed out",
                        ))
                    })
            }
        };

        if let Some(policy) = policy {
            policy.execute(execute).await
        } else {
            execute().await
        }
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
    use rskit_errors::AppError;
    use rskit_hook::HookError;
    use rskit_llm::types;
    use rskit_resilience::{ConstantBackoff, RetryPolicy};
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
            .register(from_fn(
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
            ))
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
    async fn test_agent_hook_fatal_error_stops() {
        let provider = Arc::new(MockProvider::single_text("Hello"));
        let hooks = Arc::new(HookRegistry::new());

        let _unsub = hooks
            .on::<crate::hooks::TurnStart>(0, |_, _| Err(HookError::fatal("blocked by policy")));

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

        let _unsub = hooks.on::<crate::hooks::PreLLMCall>(0, move |_, event| {
            let call = event
                .as_any()
                .downcast_ref::<crate::hooks::PreLLMCall>()
                .expect("pre LLM call event");
            assert_eq!(call.request.model, "test-model");
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
        let _unsub1 = hooks.on::<crate::hooks::PreLLMCall>(0, move |_, _| {
            pc.fetch_add(1, Ordering::SeqCst);
            Ok(())
        });

        let poc = post_count.clone();
        let _unsub2 = hooks.on::<crate::hooks::PostLLMCall>(0, move |_, _| {
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
