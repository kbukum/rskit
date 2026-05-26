//! Tool execution, timeout, retry, and context construction.

use std::time::Duration;

use rskit_errors::AppResult;
use rskit_resilience::Policy;
use rskit_tool::{Context, Registry, ToolInput, ToolResult};

pub(crate) async fn execute_tool_call(
    tools: &Registry,
    policy: Option<Policy>,
    timeout: Duration,
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
            tokio::time::timeout(timeout, tools.call(&tool_name, &ctx, input))
                .await
                .unwrap_or_else(|_| {
                    Err(rskit_errors::AppError::new(
                        rskit_errors::ErrorCode::Timeout,
                        "tool call timed out",
                    ))
                })
        }
    };

    if let Some(policy) = policy {
        policy.execute(execute).await
    } else {
        execute().await
    }
}
