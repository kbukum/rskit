use rskit_component::{Component, Health};
use rskit_errors::AppResult;

use super::Agent;

#[async_trait::async_trait]
impl Component for Agent {
    fn name(&self) -> &'static str {
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
