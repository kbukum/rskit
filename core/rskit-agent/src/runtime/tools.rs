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
                let timeout = self.timeout;
                tokio::time::timeout(timeout, self.tools.call(&tool_name, &ctx, input))
                    .await
                    .unwrap_or_else(|_| {
                        Err(rskit_errors::AppError::new(
                            rskit_errors::ErrorCode::Timeout,
                            format!("tool call to '{tool_name}' timed out after {timeout:?}"),
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

#[cfg(test)]
mod tests {
    use super::*;

    use rskit_errors::ErrorCode;
    use rskit_tool::{Context, from_fn};
    use schemars::JsonSchema;
    use serde::Deserialize;

    #[derive(Deserialize, JsonSchema)]
    struct EmptyInput {}

    #[tokio::test]
    async fn timeout_error_carries_tool_name_and_duration() {
        let registry = Registry::new();
        registry
            .register(
                from_fn(
                    "slow_tool",
                    "A tool that never completes in time",
                    move |_ctx: Context, _input: EmptyInput| async move {
                        tokio::time::sleep(Duration::from_mins(1)).await;
                        Ok(rskit_tool::text_result("done"))
                    },
                )
                .unwrap(),
            )
            .unwrap();

        let execution = ToolExecution {
            tools: &registry,
            policy: None,
            timeout: Duration::from_millis(50),
        };

        let err = execution
            .execute("tc_1", "slow_tool", ToolInput::empty())
            .await
            .expect_err("slow tool should time out");

        assert_eq!(err.code(), ErrorCode::Timeout);
        assert!(
            err.message().contains("slow_tool"),
            "timeout message must name the tool, got: {}",
            err.message()
        );
        assert!(
            err.message().contains("50ms"),
            "timeout message must include the timeout duration, got: {}",
            err.message()
        );
    }
}
