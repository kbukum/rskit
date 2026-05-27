use std::pin::Pin;

use async_stream::stream;
use futures::Stream;
use rskit_hook::CancellationToken;
use rskit_llm::types::{Message, Usage};

use super::Agent;
use crate::runtime::state::RunState;
use crate::runtime::stop;
use crate::runtime::turn::{self, TurnOutcome};
use crate::types::{AgentEvent, StopReason};

impl Agent {
    /// Stream the agent loop, yielding [`AgentEvent`]s for each lifecycle point.
    pub fn stream(
        &self,
        messages: Vec<Message>,
    ) -> Pin<Box<dyn Stream<Item = AgentEvent> + Send + '_>> {
        Box::pin(stream! {
            let mut state = RunState::new(&self.config.system_prompt, messages);

            if let Some(stop_reason) = stop::initial_stop(&self.config) {
                yield AgentEvent::Complete {
                    result: state.finish(0, stop_reason),
                };
                return;
            }

            let hook_token = CancellationToken::new();

            for turn in 0..self.config.max_turns {
                if let Some(stop_reason) = stop::wall_clock_stop(&state, &self.config) {
                    yield AgentEvent::Complete {
                        result: state.finish(turn, stop_reason),
                    };
                    return;
                }

                let usage_before = state.total_usage;
                yield AgentEvent::TurnStart { turn };

                let outcome = match turn::run_turn(
                    self.provider.as_ref(),
                    &self.config,
                    &mut state,
                    turn,
                    &hook_token,
                )
                .await
                {
                    Ok(outcome) => outcome,
                    Err(error) => {
                        tracing::error!(error = %error, "agent.run.failed");
                        yield AgentEvent::Complete {
                            result: state.finish(turn, StopReason::Aborted),
                        };
                        return;
                    }
                };

                let usage = usage_delta(usage_before, state.total_usage);
                let completed_turn = match &outcome {
                    TurnOutcome::Continue => true,
                    TurnOutcome::Stop { turn_count, .. } => *turn_count > turn,
                };

                if completed_turn {
                    yield AgentEvent::TurnComplete {
                        turn,
                        message: state.last_assistant.clone(),
                        usage,
                    };
                }

                match outcome {
                    TurnOutcome::Continue => {}
                    TurnOutcome::Stop { turn_count, reason } => {
                        yield AgentEvent::Complete {
                            result: state.finish(turn_count, reason),
                        };
                        return;
                    }
                }
            }

            yield AgentEvent::Complete {
                result: state.finish(self.config.max_turns, StopReason::MaxTurns),
            };
        })
    }
}

fn usage_delta(before: Usage, after: Usage) -> Usage {
    Usage {
        input_tokens: after.input_tokens.saturating_sub(before.input_tokens),
        output_tokens: after.output_tokens.saturating_sub(before.output_tokens),
        cached_tokens: after.cached_tokens.saturating_sub(before.cached_tokens),
        reasoning_tokens: after
            .reasoning_tokens
            .saturating_sub(before.reasoning_tokens),
    }
}

#[cfg(test)]
mod tests {
    use rskit_llm::types::Usage;

    use super::usage_delta;

    #[test]
    fn usage_delta_reports_per_turn_usage() {
        let before = Usage {
            input_tokens: 10,
            output_tokens: 5,
            cached_tokens: 2,
            reasoning_tokens: 1,
        };
        let after = Usage {
            input_tokens: 17,
            output_tokens: 9,
            cached_tokens: 3,
            reasoning_tokens: 6,
        };

        assert_eq!(
            usage_delta(before, after),
            Usage {
                input_tokens: 7,
                output_tokens: 4,
                cached_tokens: 1,
                reasoning_tokens: 5,
            }
        );
    }

    #[test]
    fn usage_delta_saturates_if_counters_reset() {
        let before = Usage {
            input_tokens: 10,
            output_tokens: 5,
            cached_tokens: 2,
            reasoning_tokens: 1,
        };
        let after = Usage {
            input_tokens: 7,
            output_tokens: 9,
            cached_tokens: 1,
            reasoning_tokens: 6,
        };

        assert_eq!(
            usage_delta(before, after),
            Usage {
                input_tokens: 0,
                output_tokens: 4,
                cached_tokens: 0,
                reasoning_tokens: 5,
            }
        );
    }
}
