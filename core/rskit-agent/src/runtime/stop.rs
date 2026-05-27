//! Stop-condition checks for the agent loop.

use rskit_errors::{AppError, ErrorCode};

use crate::config::AgentConfig;
use crate::runtime::state::RunState;
use crate::types::StopReason;

pub(crate) fn initial_stop(config: &AgentConfig) -> Option<StopReason> {
    (config.max_tokens == 0).then_some(StopReason::MaxTokens)
}

pub(crate) fn wall_clock_stop(state: &RunState, config: &AgentConfig) -> Option<StopReason> {
    (state.elapsed() >= config.wall_clock).then_some(StopReason::WallClockExceeded)
}

pub(crate) fn tool_budget_stop(state: &RunState, config: &AgentConfig) -> Option<StopReason> {
    (state.tool_calls_used >= config.max_tool_calls).then_some(StopReason::MaxToolCallsExceeded)
}

pub(crate) fn token_budget_stop(state: &RunState, config: &AgentConfig) -> Option<StopReason> {
    (state.total_tokens() >= config.max_tokens).then_some(StopReason::MaxTokens)
}

pub(crate) fn wall_clock_error() -> AppError {
    AppError::new(ErrorCode::Timeout, "agent wall-clock budget exceeded")
}
