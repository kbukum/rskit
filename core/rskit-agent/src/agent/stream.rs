use std::pin::Pin;

use async_stream::stream;
use futures::Stream;
use rskit_llm::types::Message;

use super::Agent;
use crate::types::AgentEvent;

impl Agent {
    /// Run the agent and yield a completion event when the run finishes.
    pub fn stream(
        &self,
        messages: Vec<Message>,
    ) -> Pin<Box<dyn Stream<Item = AgentEvent> + Send + '_>> {
        Box::pin(stream! {
            match self.run(messages).await {
                Ok(result) => {
                    yield AgentEvent::Complete { result };
                }
                Err(error) => {
                    tracing::error!(error = %error, "agent.run.failed");
                }
            }
        })
    }
}
