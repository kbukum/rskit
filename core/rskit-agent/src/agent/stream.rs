use std::pin::Pin;

use async_stream::stream;
use futures::Stream;
use rskit_llm::types::Message;

use super::Agent;
use crate::types::AgentEvent;

impl Agent {
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
                Err(error) => {
                    tracing::error!(error = %error, "agent.run.failed");
                }
            }
        })
    }
}
