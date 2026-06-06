//! Per-turn orchestration for the agent loop.

use rskit_errors::AppResult;
use rskit_hook::CancellationToken;
use rskit_llm::provider::Provider;
use rskit_observability::set_span_attribute;

use crate::config::AgentConfig;
use crate::hooks;
use crate::runtime::hook_dispatch::emit_hook;
use crate::runtime::llm::{self, LlmCallOutcome};
use crate::runtime::state::RunState;
use crate::runtime::{stop, tool_calls};
use crate::types::StopReason;

pub(crate) enum TurnOutcome {
    Continue,
    Stop { turn_count: u32, reason: StopReason },
}

impl TurnOutcome {
    pub(crate) const fn stop(turn_count: u32, reason: StopReason) -> Self {
        Self::Stop { turn_count, reason }
    }
}

pub(crate) async fn run_turn(
    provider: &dyn Provider,
    config: &AgentConfig,
    state: &mut RunState,
    turn: u32,
    hook_token: &CancellationToken,
) -> AppResult<TurnOutcome> {
    let turn_span = tracing::info_span!(
        "agent.turn",
        "gen_ai.operation.name" = rskit_ai::semconv::Operation::AgentTurn.as_str(),
        "agent.turn" = turn,
    );
    set_span_attribute(
        &turn_span,
        rskit_ai::semconv::OPERATION_NAME,
        rskit_ai::semconv::Operation::AgentTurn.as_str(),
    );

    if let Some(stop_reason) = stop::wall_clock_stop(state, config) {
        return Ok(TurnOutcome::stop(turn, stop_reason));
    }

    if let Some(ref hooks) = config.hooks
        && emit_hook(hooks, &hooks::TurnStart { turn }, hook_token.clone())
    {
        return Ok(TurnOutcome::stop(turn, StopReason::Aborted));
    }

    match llm::complete_turn(provider, config, state, turn_span.clone(), hook_token).await? {
        LlmCallOutcome::AbortedBeforeCall => {
            return Ok(TurnOutcome::stop(turn, StopReason::Aborted));
        }
        LlmCallOutcome::AbortedAfterResponse(response) => {
            state.record_response(response);
            return Ok(TurnOutcome::stop(turn + 1, StopReason::Aborted));
        }
        LlmCallOutcome::Completed {
            response,
            has_tool_calls,
            stop_reason,
        } => {
            state.record_response(response);

            if let Some(stop_reason) = stop::token_budget_stop(state, config) {
                return Ok(TurnOutcome::stop(turn + 1, stop_reason));
            }

            if !has_tool_calls {
                return Ok(TurnOutcome::stop(turn + 1, StopReason::from(stop_reason)));
            }
        }
    }

    if let Some(stop_reason) =
        tool_calls::execute_requested_tools(config, state, turn_span, hook_token).await?
    {
        return Ok(TurnOutcome::stop(turn + 1, stop_reason));
    }

    let caps = provider.capabilities();
    state.compact_context(caps.max_input_tokens, config.context_strategy.as_deref())?;

    if let Some(ref hooks) = config.hooks
        && emit_hook(
            hooks,
            &hooks::TurnEnd {
                turn,
                message: state.last_assistant.clone(),
            },
            hook_token.clone(),
        )
    {
        return Ok(TurnOutcome::stop(turn + 1, StopReason::Aborted));
    }

    Ok(TurnOutcome::Continue)
}
