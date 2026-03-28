//! Source trait — pull data from any origin.

use crate::DataItem;
use rskit_cli::CancellationToken;
use rskit_errors::AppResult;

/// Protocol for dataset sources.
#[async_trait::async_trait]
pub trait Source: Send + Sync {
    fn name(&self) -> &str;

    fn display_name(&self) -> &str {
        self.name().rsplit('/').next().unwrap_or(self.name())
    }

    async fn fetch(
        &self,
        cancel: &CancellationToken,
        on_item: &mut (dyn FnMut(DataItem) -> bool + Send),
    ) -> AppResult<usize>;

    fn cache_key(&self) -> serde_json::Value;
    fn max_items(&self) -> Option<usize>;

    fn set_resume_state(&mut self, _offset: usize, _already_fetched: usize) {}
    fn last_offset(&self) -> usize {
        0
    }
}
