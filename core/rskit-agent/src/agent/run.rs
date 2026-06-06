use rskit_ai::semconv;
use rskit_errors::AppResult;
use rskit_hook::CancellationToken;
use rskit_llm::types::Message;
use rskit_observability::set_span_attribute;
use tracing::Instrument;

use super::Agent;
use crate::runtime::state::RunState;
use crate::runtime::stop;
use crate::runtime::turn::{self, TurnOutcome};
use crate::types::{AgentResult, StopReason};

impl Agent {
    /// Run the agent loop synchronously (all turns, no streaming).
    pub async fn run(&self, messages: Vec<Message>) -> AppResult<AgentResult> {
        let run_span = tracing::info_span!(
            "agent.run",
            "gen_ai.system" = "agent",
            "gen_ai.operation.name" = semconv::Operation::AgentRun.as_str(),
            "gen_ai.request.model" = %self.config.model,
        );
        set_span_attribute(&run_span, semconv::SYSTEM, "agent");
        set_span_attribute(
            &run_span,
            semconv::OPERATION_NAME,
            semconv::Operation::AgentRun.as_str(),
        );
        set_span_attribute(
            &run_span,
            semconv::REQUEST_MODEL,
            self.config.model.as_str(),
        );

        async move {
            let mut state = RunState::new(&self.config.system_prompt, messages);

            if let Some(stop_reason) = stop::initial_stop(&self.config) {
                return Ok(state.finish(0, stop_reason));
            }

            let hook_token = CancellationToken::new();

            for turn in 0..self.config.max_turns {
                match turn::run_turn(
                    self.provider.as_ref(),
                    &self.config,
                    &mut state,
                    turn,
                    &hook_token,
                )
                .await?
                {
                    TurnOutcome::Continue => {}
                    TurnOutcome::Stop { turn_count, reason } => {
                        return Ok(state.finish(turn_count, reason));
                    }
                }
            }

            Ok(state.finish(self.config.max_turns, StopReason::MaxTurns))
        }
        .instrument(run_span)
        .await
    }
}
