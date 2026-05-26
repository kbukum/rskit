//! LLM call orchestration for a single agent turn.

use rskit_errors::AppResult;
use rskit_hook::CancellationToken;
use rskit_llm::provider::Provider;
use rskit_llm::types::CompletionResponse;
use tracing::Instrument;

use crate::config::AgentConfig;
use crate::hooks;
use crate::runtime::hook_dispatch::emit_hook;
use crate::runtime::request::build_completion_request;
use crate::runtime::state::RunState;
use crate::runtime::stop;

pub(crate) enum LlmCallOutcome {
    AbortedBeforeCall,
    AbortedAfterResponse(CompletionResponse),
    Completed {
        response: CompletionResponse,
        has_tool_calls: bool,
        stop_reason: rskit_llm::FinishReason,
    },
}

pub(crate) async fn complete_turn(
    provider: &dyn Provider,
    config: &AgentConfig,
    state: &RunState,
    turn_span: tracing::Span,
    hook_token: &CancellationToken,
) -> AppResult<LlmCallOutcome> {
    let request = build_completion_request(config, &state.messages);

    if let Some(ref hooks) = config.hooks
        && emit_hook(
            hooks,
            &hooks::PreLLMCall {
                request: request.clone(),
            },
            hook_token.clone(),
        )
    {
        return Ok(LlmCallOutcome::AbortedBeforeCall);
    }

    let response: CompletionResponse = tokio::time::timeout(
        state.remaining_wall_clock(config.wall_clock),
        provider.complete(request),
    )
    .instrument(turn_span)
    .await
    .map_err(|_| stop::wall_clock_error())??;

    if let Some(ref hooks) = config.hooks
        && emit_hook(
            hooks,
            &hooks::PostLLMCall {
                response: response.clone(),
                error: None,
            },
            hook_token.clone(),
        )
    {
        return Ok(LlmCallOutcome::AbortedAfterResponse(response));
    }

    let stop_reason = response
        .stop_reason
        .unwrap_or(rskit_llm::FinishReason::Stop);
    let has_tool_calls = response.has_tool_calls();
    Ok(LlmCallOutcome::Completed {
        response,
        has_tool_calls,
        stop_reason,
    })
}
