//! Tool-call orchestration for a single agent turn.

use rskit_errors::AppResult;
use rskit_hook::CancellationToken;
use rskit_tool::{ToolInput, ToolResult};
use tracing::Instrument;

use crate::config::AgentConfig;
use crate::hooks;
use crate::runtime::hook_dispatch::emit_hook;
use crate::runtime::state::RunState;
use crate::runtime::{stop, tools as tool_runtime};
use crate::types::StopReason;

pub(crate) async fn execute_requested_tools(
    config: &AgentConfig,
    state: &mut RunState,
    turn_span: tracing::Span,
    hook_token: &CancellationToken,
) -> AppResult<Option<StopReason>> {
    let Some(ref tools) = config.tools else {
        return Ok(None);
    };

    let tool_calls = state.last_assistant.tool_calls.clone();
    for tool_call in &tool_calls {
        if let Some(stop_reason) = stop::tool_budget_stop(state, config) {
            return Ok(Some(stop_reason));
        }

        state.tool_calls_used += 1;
        let input = ToolInput::new(serde_json::Value::Object(tool_call.input.clone()))?;

        if let Some(ref hooks) = config.hooks
            && emit_hook(
                hooks,
                &hooks::PreToolCall {
                    name: tool_call.name.clone(),
                    input: input.clone(),
                },
                hook_token.clone(),
            )
        {
            return Ok(Some(StopReason::Aborted));
        }

        let tool_result = tool_runtime::execute_tool_call(
            tools,
            config.policy.clone(),
            config.tool_timeout,
            &tool_call.id,
            &tool_call.name,
            input.clone(),
        )
        .instrument(turn_span.clone())
        .await;

        emit_tool_result_hook(
            config,
            tool_call.name.clone(),
            input,
            &tool_result,
            hook_token,
        );
        append_tool_result(state, &tool_call.id, tool_result);
    }

    Ok(None)
}

fn emit_tool_result_hook(
    config: &AgentConfig,
    name: String,
    input: ToolInput,
    tool_result: &AppResult<ToolResult>,
    hook_token: &CancellationToken,
) {
    let Some(ref hooks) = config.hooks else {
        return;
    };

    let (result, error): (Option<ToolResult>, Option<String>) = match tool_result {
        Ok(result) => (Some(result.clone()), None),
        Err(error) => (None, Some(error.to_string())),
    };

    let _ = emit_hook(
        hooks,
        &hooks::PostToolCall {
            name,
            input,
            result,
            error,
        },
        hook_token.clone(),
    );
}

fn append_tool_result(state: &mut RunState, tool_use_id: &str, tool_result: AppResult<ToolResult>) {
    let (content, is_error) = match tool_result {
        Ok(result) => (result.content, result.is_error),
        Err(error) => (error.to_string(), true),
    };

    state
        .messages
        .push(rskit_llm::tool_result_msg(tool_use_id, &content, is_error));
}
