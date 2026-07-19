//! Tool execution, timeout, retry, and context construction.

use std::time::Duration;

use rskit_errors::AppResult;
use rskit_resilience::Policy;
use rskit_tool::{Context, Registry, ToolInput, ToolResult};

/// Execution environment for the tool calls in a single agent turn — the tool
/// registry, optional resilience policy, and per-call timeout stay constant
/// across the turn and are grouped here rather than threaded positionally.
pub(crate) struct ToolExecution<'a> {
    pub(crate) tools: &'a Registry,
    pub(crate) policy: Option<Policy>,
    pub(crate) timeout: Duration,
}

impl ToolExecution<'_> {
    pub(crate) async fn execute(
        &self,
        tool_use_id: &str,
        name: &str,
        input: ToolInput,
    ) -> AppResult<ToolResult> {
        let tool_name = name.to_string();
        let tool_use_id = tool_use_id.to_string();

        let execute = || {
            let tool_name = tool_name.clone();
            let tool_use_id = tool_use_id.clone();
            let input = input.clone();
            async move {
                let mut ctx = Context::new();
                ctx.tool_use_id = tool_use_id;
                tokio::time::timeout(self.timeout, self.tools.call(&tool_name, &ctx, input))
                    .await
                    .unwrap_or_else(|_| {
                        Err(rskit_errors::AppError::new(
                            rskit_errors::ErrorCode::Timeout,
                            "tool call timed out",
                        ))
                    })
            }
        };

        if let Some(policy) = &self.policy {
            policy.execute(execute).await
        } else {
            execute().await
        }
    }
}
