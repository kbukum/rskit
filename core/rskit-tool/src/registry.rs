//! Concurrent-safe tool registry.

use parking_lot::RwLock;
use rskit_ai::semconv;
use rskit_component::{Component, Health};
use rskit_errors::{AppError, AppResult, ErrorCode};
use rskit_observability::set_span_attribute;
use std::collections::HashMap;
use std::sync::Arc;
use tracing::Instrument;

use crate::callable::Callable;
use crate::context::Context;
use crate::definition::{Definition, ExecutionHint};
use crate::hitl::{Decision, HumanApproval, SensitivityEvaluator, ToolCall, denied_error};
use crate::io::ToolInput;
use crate::result::ToolResult;

/// Options for passive batch tool execution.
#[derive(Debug, Clone, Copy)]
pub struct BatchOptions {
    /// Maximum number of concurrent calls. Values below 1 are treated as 1.
    pub concurrency: usize,
    /// Stop scheduling calls after the first error.
    pub fail_fast: bool,
}

impl Default for BatchOptions {
    fn default() -> Self {
        Self {
            concurrency: 1,
            fail_fast: true,
        }
    }
}

/// Thread-safe registry of callable tools.
///
/// Optionally wires the HITL stages (per locked decision D10):
/// `sensitivity → (if RequireApproval) human approval → invoke`.
/// The authorization stage is owned by the boundary (`rskit-mcp`, etc.) and is not enforced here;
/// this preserves module layering.
pub struct Registry {
    tools: RwLock<HashMap<String, Arc<dyn Callable>>>,
    sensitivity: Option<Arc<dyn SensitivityEvaluator>>,
    approval: Option<Arc<dyn HumanApproval>>,
}

impl Registry {
    /// Create a new empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self {
            tools: RwLock::new(HashMap::new()),
            sensitivity: None,
            approval: None,
        }
    }

    /// Inject a sensitivity evaluator. When unset, no sensitivity checks run.
    #[must_use]
    pub fn with_sensitivity_evaluator(mut self, evaluator: Arc<dyn SensitivityEvaluator>) -> Self {
        self.sensitivity = Some(evaluator);
        self
    }

    /// Inject a human-approval gate. When unset, `RequireApproval` decisions are treated as denials.
    #[must_use]
    pub fn with_human_approval(mut self, approval: Arc<dyn HumanApproval>) -> Self {
        self.approval = Some(approval);
        self
    }

    /// Register a tool. Returns error on empty name or duplicate.
    pub fn register(&self, tool: Box<dyn Callable>) -> AppResult<()> {
        let name = tool.definition().name.clone();
        if name.trim().is_empty() {
            return Err(AppError::new(
                ErrorCode::InvalidInput,
                "tool name must not be empty",
            ));
        }
        let mut tools = self.tools.write();
        if tools.contains_key(&name) {
            return Err(AppError::new(
                ErrorCode::AlreadyExists,
                format!("tool already registered: {name:?}"),
            ));
        }
        tools.insert(name, Arc::from(tool));
        drop(tools);
        Ok(())
    }

    /// Get a tool by name.
    pub fn get(&self, name: &str) -> Option<Arc<dyn Callable>> {
        self.tools.read().get(name).cloned()
    }

    /// List all registered tool definitions.
    pub fn list(&self) -> Vec<Definition> {
        self.tools
            .read()
            .values()
            .map(|t| t.definition().clone())
            .collect()
    }

    /// List all registered tool names.
    pub fn names(&self) -> Vec<String> {
        self.tools.read().keys().cloned().collect()
    }

    /// Call a tool by name with a context.
    /// Runs the HITL stages (sensitivity → human approval) if configured before invoking the tool.
    pub async fn call(&self, name: &str, ctx: &Context, input: ToolInput) -> AppResult<ToolResult> {
        self.call_inner(name, ctx, input, true).await
    }

    /// Call a tool after the caller has already validated the input schema.
    ///
    /// This still runs HITL stages before invoking the tool,
    /// but skips the schema validation pass performed by [`Registry::call`].
    pub async fn call_validated(
        &self,
        name: &str,
        ctx: &Context,
        input: ToolInput,
    ) -> AppResult<ToolResult> {
        self.call_inner(name, ctx, input, false).await
    }

    async fn call_inner(
        &self,
        name: &str,
        ctx: &Context,
        input: ToolInput,
        validate_input: bool,
    ) -> AppResult<ToolResult> {
        let span = tracing::info_span!(
            "tool.call",
            "gen_ai.operation.name" = semconv::Operation::ToolCall.as_str(),
            "gen_ai.tool.name" = name,
            "tool.use_id" = %ctx.tool_use_id,
        );
        set_span_attribute(
            &span,
            semconv::OPERATION_NAME,
            semconv::Operation::ToolCall.as_str(),
        );
        set_span_attribute(&span, semconv::TOOL_NAME, name);
        async {
            let tool = self.get(name).ok_or_else(|| {
                AppError::new(ErrorCode::NotFound, format!("tool not found: {name:?}"))
            })?;
            if validate_input {
                validate_tool_input(tool.as_ref(), &input)?;
            }
            self.run_hitl(tool.as_ref(), ctx, &input).await?;
            tool.call(ctx, input).await
        }
        .instrument(span)
        .await
    }

    async fn run_hitl(
        &self,
        tool: &dyn Callable,
        ctx: &Context,
        input: &ToolInput,
    ) -> AppResult<()> {
        let evaluator = match &self.sensitivity {
            Some(e) => e.clone(),
            None => return Ok(()),
        };
        let definition = tool.definition();
        let call = ToolCall {
            name: definition.name.clone(),
            input: input.clone(),
        };
        let decision = evaluator.evaluate(ctx, &call, &definition.envelope).await?;
        match decision {
            Decision::Allow => Ok(()),
            Decision::Deny(reason) => Err(denied_error(reason)),
            Decision::RequireApproval(reason) => match &self.approval {
                Some(approver) => {
                    if approver.approve(ctx, &call, &reason).await? {
                        Ok(())
                    } else {
                        Err(denied_error(format!("human approval rejected: {reason}")))
                    }
                }
                None => Err(denied_error(format!(
                    "approval required but no approver configured: {reason}"
                ))),
            },
        }
    }

    /// Search for tools whose name or description contains the query (case-insensitive).
    pub fn search(&self, query: &str) -> Vec<Definition> {
        let q = query.to_lowercase();
        self.tools
            .read()
            .values()
            .filter_map(|t| {
                let def = t.definition();
                if def.name.to_lowercase().contains(&q)
                    || def.description.to_lowercase().contains(&q)
                {
                    Some(def.clone())
                } else {
                    None
                }
            })
            .collect()
    }

    /// Filter tools by `execution_hint` annotation.
    pub fn filter_by_execution_hint(&self, hint: ExecutionHint) -> Vec<Definition> {
        self.tools
            .read()
            .values()
            .filter_map(|t| {
                let def = t.definition();
                (def.annotations.execution_hint.effective() == hint.effective())
                    .then(|| def.clone())
            })
            .collect()
    }

    /// Call multiple tools with caller-supplied concurrency policy.
    /// Each call goes through the same HITL stages as [`Registry::call`].
    pub async fn call_batch(
        &self,
        calls: Vec<(&str, ToolInput)>,
        ctx: &Context,
        options: BatchOptions,
    ) -> Vec<AppResult<ToolResult>> {
        let concurrency = options.concurrency.max(1);
        let mut results = Vec::with_capacity(calls.len());

        for chunk in calls.chunks(concurrency) {
            let mut handles = Vec::with_capacity(chunk.len());
            for (name, input) in chunk {
                let name = (*name).to_string();
                let input = input.clone();
                let ctx = ctx.clone();
                let tool = self.get(&name);
                let sensitivity = self.sensitivity.clone();
                let approval = self.approval.clone();
                let span = tracing::info_span!(
                    "tool.call",
                    "gen_ai.operation.name" = semconv::Operation::ToolCall.as_str(),
                    "gen_ai.tool.name" = name.as_str(),
                    "tool.use_id" = %ctx.tool_use_id,
                );
                set_span_attribute(
                    &span,
                    semconv::OPERATION_NAME,
                    semconv::Operation::ToolCall.as_str(),
                );
                set_span_attribute(&span, semconv::TOOL_NAME, name.as_str());
                handles.push(tokio::spawn(
                    async move {
                        let Some(tool) = tool else {
                            return Err(AppError::new(
                                ErrorCode::NotFound,
                                format!("tool not found: {name:?}"),
                            ));
                        };
                        validate_tool_input(tool.as_ref(), &input)?;
                        if let Some(evaluator) = sensitivity {
                            let definition = tool.definition();
                            let call = ToolCall {
                                name: definition.name.clone(),
                                input: input.clone(),
                            };
                            let decision = evaluator
                                .evaluate(&ctx, &call, &definition.envelope)
                                .await?;
                            match decision {
                                Decision::Allow => {}
                                Decision::Deny(reason) => return Err(denied_error(reason)),
                                Decision::RequireApproval(reason) => match approval {
                                    Some(approver) => {
                                        if !approver.approve(&ctx, &call, &reason).await? {
                                            return Err(denied_error(format!(
                                                "human approval rejected: {reason}"
                                            )));
                                        }
                                    }
                                    None => {
                                        return Err(denied_error(format!(
                                            "approval required but no approver configured: {reason}"
                                        )));
                                    }
                                },
                            }
                        }
                        tool.call(&ctx, input).await
                    }
                    .instrument(span),
                ));
            }

            for handle in handles {
                let result = match handle.await {
                    Ok(result) => result,
                    Err(error) => Err(AppError::new(
                        ErrorCode::Internal,
                        format!("task join error: {error}"),
                    )),
                };
                let failed = result.is_err();
                results.push(result);
                if failed && options.fail_fast {
                    return results;
                }
            }
        }

        results
    }

    /// Number of registered tools.
    pub fn len(&self) -> usize {
        self.tools.read().len()
    }

    /// Whether the registry is empty.
    pub fn is_empty(&self) -> bool {
        self.tools.read().is_empty()
    }

    /// Check if a tool is registered.
    pub fn contains(&self, name: &str) -> bool {
        self.tools.read().contains_key(name)
    }
}

fn validate_tool_input(tool: &dyn Callable, input: &ToolInput) -> AppResult<()> {
    let validation = tool.validate(input);
    if validation.valid {
        return Ok(());
    }
    let mut details = validation
        .errors
        .iter()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join("; ");
    if details.is_empty() {
        details = String::from("schema validation failed");
    }
    Err(AppError::new(
        ErrorCode::InvalidInput,
        format!(
            "invalid tool input for {}: {details}",
            tool.definition().name
        ),
    ))
}

#[async_trait::async_trait]
impl Component for Registry {
    fn name(&self) -> &'static str {
        "rskit-tool.registry"
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

impl Default for Registry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::definition::Definition;
    use crate::envelope::{Envelope, SensitiveMatcher, SensitivePredicate};
    use crate::hitl::{DenyHumanApproval, DenyOnSensitive};
    use crate::result::ToolResult;
    use serde_json::json;

    struct StubTool {
        def: Definition,
        valid: bool,
    }

    #[async_trait::async_trait]
    impl Callable for StubTool {
        fn definition(&self) -> &Definition {
            &self.def
        }
        fn validate(&self, _input: &ToolInput) -> rskit_schema::ValidationResult {
            rskit_schema::ValidationResult {
                valid: self.valid,
                errors: Vec::new(),
            }
        }
        async fn call(&self, _ctx: &Context, input: ToolInput) -> AppResult<ToolResult> {
            Ok(ToolResult {
                output: Some(crate::ToolOutput::from(input.into_json())),
                content: "ok".to_owned(),
                is_error: false,
                metadata: crate::ToolMetadata::new(),
            })
        }
    }

    fn stub(name: &str, env: Envelope) -> Box<dyn Callable> {
        Box::new(StubTool {
            def: Definition {
                name: name.to_owned(),
                description: "stub".to_owned(),
                input_schema: crate::ToolSchema::new(json!({"type": "object"})).unwrap(),
                output_schema: None,
                annotations: crate::Annotations::default(),
                envelope: env,
            },
            valid: true,
        })
    }

    fn invalid_stub(name: &str) -> Box<dyn Callable> {
        Box::new(StubTool {
            def: Definition {
                name: name.to_owned(),
                description: "stub".to_owned(),
                input_schema: crate::ToolSchema::new(json!({"type": "object"})).unwrap(),
                output_schema: None,
                annotations: crate::Annotations::default(),
                envelope: Envelope::default(),
            },
            valid: false,
        })
    }

    #[tokio::test]
    async fn register_rejects_empty_name() {
        let registry = Registry::new();
        let err = registry
            .register(stub("", Envelope::default()))
            .expect_err("empty name rejected");
        assert_eq!(err.code(), ErrorCode::InvalidInput);
    }

    #[tokio::test]
    async fn deny_on_sensitive_blocks_dispatch() {
        let env = Envelope {
            sensitive_invocations: vec![SensitivePredicate {
                jsonpath: "$.msg".to_owned(),
                matcher: SensitiveMatcher::Exists,
            }],
            ..Envelope::default()
        };
        let registry = Registry::new().with_sensitivity_evaluator(Arc::new(DenyOnSensitive));
        registry.register(stub("danger", env)).unwrap();

        let ctx = Context::new();
        let err = registry
            .call(
                "danger",
                &ctx,
                ToolInput::new(json!({"msg": "hi"})).unwrap(),
            )
            .await
            .expect_err("sensitive call denied");
        assert_eq!(err.code(), ErrorCode::Forbidden);
    }

    #[tokio::test]
    async fn allow_when_no_sensitive_predicate_matches() {
        let registry = Registry::new().with_sensitivity_evaluator(Arc::new(DenyOnSensitive));
        registry
            .register(stub("safe", Envelope::default()))
            .unwrap();

        let ctx = Context::new();
        let result = registry
            .call("safe", &ctx, ToolInput::new(json!({"msg": "hi"})).unwrap())
            .await
            .unwrap();
        assert!(!result.is_error);
    }

    #[tokio::test]
    async fn require_approval_with_deny_human_rejects() {
        struct AlwaysApprove;
        #[async_trait::async_trait]
        impl SensitivityEvaluator for AlwaysApprove {
            async fn evaluate(
                &self,
                _ctx: &Context,
                _call: &ToolCall,
                _envelope: &Envelope,
            ) -> AppResult<Decision> {
                Ok(Decision::RequireApproval("policy".into()))
            }
        }
        let registry = Registry::new()
            .with_sensitivity_evaluator(Arc::new(AlwaysApprove))
            .with_human_approval(Arc::new(DenyHumanApproval));
        registry.register(stub("any", Envelope::default())).unwrap();

        let ctx = Context::new();
        let err = registry
            .call("any", &ctx, ToolInput::new(json!({"msg": "x"})).unwrap())
            .await
            .expect_err("denied by human approval default");
        assert_eq!(err.code(), ErrorCode::Forbidden);
    }

    #[tokio::test]
    async fn invalid_input_without_details_uses_fallback_message() {
        let registry = Registry::new();
        registry
            .register(invalid_stub("broken"))
            .expect("register invalid stub");

        let err = registry
            .call("broken", &Context::new(), ToolInput::empty())
            .await
            .expect_err("invalid input rejected");

        assert_eq!(
            err.message(),
            "invalid tool input for broken: schema validation failed"
        );
    }

    #[tokio::test]
    async fn call_validated_skips_schema_validation() {
        let registry = Registry::new();
        registry
            .register(invalid_stub("prechecked"))
            .expect("register invalid stub");

        let result = registry
            .call_validated("prechecked", &Context::new(), ToolInput::empty())
            .await
            .expect("prevalidated call skips schema validation");

        assert!(!result.is_error);
    }

    #[tokio::test]
    async fn call_batch_clamps_zero_concurrency_and_continues_when_not_fail_fast() {
        let registry = Registry::new();
        registry.register(stub("ok", Envelope::default())).unwrap();
        registry.register(invalid_stub("invalid")).unwrap();

        let results = registry
            .call_batch(
                vec![
                    ("missing", ToolInput::empty()),
                    ("ok", ToolInput::new(json!({"id": 1})).unwrap()),
                    ("invalid", ToolInput::empty()),
                ],
                &Context::new(),
                BatchOptions {
                    concurrency: 0,
                    fail_fast: false,
                },
            )
            .await;

        assert_eq!(results.len(), 3);
        assert_eq!(results[0].as_ref().unwrap_err().code(), ErrorCode::NotFound);
        assert_eq!(
            results[1].as_ref().unwrap().output.as_ref().unwrap()["id"],
            1
        );
        assert_eq!(
            results[2].as_ref().unwrap_err().code(),
            ErrorCode::InvalidInput
        );
    }

    #[tokio::test]
    async fn call_batch_stops_after_first_failure_when_fail_fast() {
        let registry = Registry::new();
        registry.register(stub("ok", Envelope::default())).unwrap();

        let results = registry
            .call_batch(
                vec![
                    ("missing", ToolInput::empty()),
                    ("ok", ToolInput::new(json!({"id": 1})).unwrap()),
                ],
                &Context::new(),
                BatchOptions {
                    concurrency: 1,
                    fail_fast: true,
                },
            )
            .await;

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].as_ref().unwrap_err().code(), ErrorCode::NotFound);
    }

    #[tokio::test]
    async fn call_requires_approval_denies_without_approver_and_allows_with_approval() {
        struct NeedsApproval;

        #[async_trait::async_trait]
        impl SensitivityEvaluator for NeedsApproval {
            async fn evaluate(
                &self,
                _ctx: &Context,
                _call: &ToolCall,
                _envelope: &Envelope,
            ) -> AppResult<Decision> {
                Ok(Decision::RequireApproval("sensitive".to_owned()))
            }
        }

        struct AllowApproval;

        #[async_trait::async_trait]
        impl HumanApproval for AllowApproval {
            async fn approve(
                &self,
                _ctx: &Context,
                _call: &ToolCall,
                reason: &str,
            ) -> AppResult<bool> {
                Ok(reason == "sensitive")
            }
        }

        let denied = Registry::new().with_sensitivity_evaluator(Arc::new(NeedsApproval));
        denied.register(stub("tool", Envelope::default())).unwrap();
        let err = denied
            .call("tool", &Context::new(), ToolInput::empty())
            .await
            .unwrap_err();
        assert_eq!(err.code(), ErrorCode::Forbidden);
        assert!(
            err.message()
                .contains("approval required but no approver configured")
        );

        let allowed = Registry::new()
            .with_sensitivity_evaluator(Arc::new(NeedsApproval))
            .with_human_approval(Arc::new(AllowApproval));
        allowed.register(stub("tool", Envelope::default())).unwrap();
        let result = allowed
            .call("tool", &Context::new(), ToolInput::empty())
            .await
            .unwrap();
        assert!(!result.is_error);
    }

    #[tokio::test]
    async fn batch_hitl_covers_deny_and_approval_paths() {
        struct NeedsApproval;

        #[async_trait::async_trait]
        impl SensitivityEvaluator for NeedsApproval {
            async fn evaluate(
                &self,
                _ctx: &Context,
                _call: &ToolCall,
                _envelope: &Envelope,
            ) -> AppResult<Decision> {
                Ok(Decision::RequireApproval("batch".to_owned()))
            }
        }

        struct AllowApproval;

        #[async_trait::async_trait]
        impl HumanApproval for AllowApproval {
            async fn approve(
                &self,
                _ctx: &Context,
                _call: &ToolCall,
                reason: &str,
            ) -> AppResult<bool> {
                Ok(reason == "batch")
            }
        }

        let denied = Registry::new().with_sensitivity_evaluator(Arc::new(DenyOnSensitive));
        denied
            .register(stub(
                "danger",
                Envelope {
                    sensitive_invocations: vec![SensitivePredicate {
                        jsonpath: "$.secret".to_owned(),
                        matcher: SensitiveMatcher::Exists,
                    }],
                    ..Envelope::default()
                },
            ))
            .unwrap();
        let denied_results = denied
            .call_batch(
                vec![("danger", ToolInput::new(json!({"secret": true})).unwrap())],
                &Context::new(),
                BatchOptions::default(),
            )
            .await;
        assert_eq!(
            denied_results[0].as_ref().unwrap_err().code(),
            ErrorCode::Forbidden
        );

        let missing_approval = Registry::new().with_sensitivity_evaluator(Arc::new(NeedsApproval));
        missing_approval
            .register(stub("tool", Envelope::default()))
            .unwrap();
        let missing_results = missing_approval
            .call_batch(
                vec![("tool", ToolInput::empty())],
                &Context::new(),
                BatchOptions::default(),
            )
            .await;
        assert_eq!(
            missing_results[0].as_ref().unwrap_err().code(),
            ErrorCode::Forbidden
        );

        let allowed = Registry::new()
            .with_sensitivity_evaluator(Arc::new(NeedsApproval))
            .with_human_approval(Arc::new(AllowApproval));
        allowed.register(stub("tool", Envelope::default())).unwrap();
        let allowed_results = allowed
            .call_batch(
                vec![("tool", ToolInput::empty())],
                &Context::new(),
                BatchOptions::default(),
            )
            .await;
        assert!(!allowed_results[0].as_ref().unwrap().is_error);
    }

    #[tokio::test]
    async fn default_registry_component_lifecycle_is_healthy() {
        let registry = Registry::default();

        assert_eq!(registry.name(), "rskit-tool.registry");
        registry.start().await.expect("start succeeds");
        assert_eq!(registry.health().name, "rskit-tool.registry");
        registry.stop().await.expect("stop succeeds");
    }
}
