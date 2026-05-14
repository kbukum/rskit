//! Tool definition, auto-wiring, registry and middleware for agentic systems.
//!
//! Provides a type-safe framework for defining tools that can be used in
//! agentic systems, LLM function calling, or MCP servers.

mod callable;
pub mod context;
mod definition;
pub mod envelope;
mod from_fn;
pub mod hitl;
pub mod registry;
pub mod result;

pub use callable::Callable;
pub use context::Context;
pub use definition::{Annotations, Definition, ExecutionHint};
pub use envelope::{
    DataClassification, Envelope, FilesystemMode, FilesystemRule, NetworkPolicy, NetworkRule,
    Safety, SensitiveMatcher, SensitivePredicate, SubprocessRule,
};
pub use from_fn::{from_fn, from_fn_simple};
pub use hitl::{
    Decision, DenyHumanApproval, DenyOnSensitive, HumanApproval, SensitivityEvaluator, ToolCall,
    denied_error,
};
pub use registry::{BatchOptions, Registry};
pub use result::{ToolResult, error_result, json_result, text_result};

// Re-export for convenience
pub use rskit_errors::{AppError, AppResult};

#[cfg(test)]
mod tests {
    use super::*;
    use schemars::JsonSchema;
    use serde::{Deserialize, Serialize};

    #[derive(Deserialize, JsonSchema)]
    struct AddInput {
        a: i32,
        b: i32,
    }

    #[derive(Serialize)]
    struct AddOutput {
        sum: i32,
    }

    #[tokio::test]
    async fn test_from_fn_basic() {
        let tool = from_fn(
            "add",
            "Add two numbers",
            |_ctx: Context, input: AddInput| async move {
                Ok(text_result(&format!("{}", input.a + input.b)))
            },
        )
        .unwrap();

        assert_eq!(tool.definition().name, "add");
        assert_eq!(tool.definition().description, "Add two numbers");

        let ctx = Context::new();
        let result = tool
            .call(&ctx, serde_json::json!({"a": 1, "b": 2}))
            .await
            .unwrap();
        assert_eq!(result.text(), "3");
    }

    #[tokio::test]
    async fn test_from_fn_simple_basic() {
        let tool = from_fn_simple("add", "Add", |input: AddInput| async move {
            Ok(AddOutput {
                sum: input.a + input.b,
            })
        })
        .unwrap();

        let ctx = Context::new();
        let result = tool
            .call(&ctx, serde_json::json!({"a": 1, "b": 2}))
            .await
            .unwrap();
        assert!(result.output.is_some());
        assert_eq!(result.output.unwrap()["sum"], 3);
    }

    #[tokio::test]
    async fn test_from_fn_schema_generated() {
        let tool = from_fn("add", "Add", |_ctx: Context, _input: AddInput| async move {
            Ok(text_result("0"))
        })
        .unwrap();

        let schema = &tool.definition().input_schema;
        assert!(schema.is_object());
        let obj = schema.as_object().unwrap();
        let props = obj.get("properties").unwrap().as_object().unwrap();
        assert!(props.contains_key("a"));
        assert!(props.contains_key("b"));
    }

    #[tokio::test]
    async fn test_from_fn_invalid_input() {
        let tool = from_fn("add", "Add", |_ctx: Context, _input: AddInput| async move {
            Ok(text_result("0"))
        })
        .unwrap();

        let ctx = Context::new();
        let result = tool.call(&ctx, serde_json::json!({"x": 1})).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_validate() {
        let tool = from_fn("add", "Add", |_ctx: Context, _input: AddInput| async move {
            Ok(text_result("0"))
        })
        .unwrap();

        let valid = tool.validate(&serde_json::json!({"a": 1, "b": 2}));
        assert!(valid.valid);

        let invalid = tool.validate(&serde_json::json!(42));
        assert!(!invalid.valid);
    }

    #[tokio::test]
    async fn test_registry_operations() {
        let registry = Registry::new();
        assert!(registry.is_empty());

        let tool = from_fn(
            "add",
            "Add two numbers",
            |_ctx: Context, input: AddInput| async move {
                Ok(text_result(&format!("{}", input.a + input.b)))
            },
        )
        .unwrap();

        registry.register(tool).unwrap();
        assert_eq!(registry.len(), 1);
        assert!(registry.contains("add"));
        assert!(!registry.contains("missing"));

        let ctx = Context::new();
        let result = registry
            .call("add", &ctx, serde_json::json!({"a": 3, "b": 4}))
            .await
            .unwrap();
        assert_eq!(result.text(), "7");
    }

    #[tokio::test]
    async fn test_registry_duplicate() {
        let registry = Registry::new();

        let t1 = from_fn("dup", "First", |_ctx: Context, _: AddInput| async move {
            Ok(text_result("1"))
        })
        .unwrap();
        let t2 = from_fn("dup", "Second", |_ctx: Context, _: AddInput| async move {
            Ok(text_result("2"))
        })
        .unwrap();

        registry.register(t1).unwrap();
        assert!(registry.register(t2).is_err());
    }

    #[tokio::test]
    async fn test_registry_not_found() {
        let registry = Registry::new();
        let ctx = Context::new();
        let result = registry.call("missing", &ctx, serde_json::json!({})).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_registry_list() {
        let registry = Registry::new();

        let t1 = from_fn("alpha", "A tool", |_ctx: Context, _: AddInput| async move {
            Ok(text_result("a"))
        })
        .unwrap();
        let t2 = from_fn("beta", "B tool", |_ctx: Context, _: AddInput| async move {
            Ok(text_result("b"))
        })
        .unwrap();

        registry.register(t1).unwrap();
        registry.register(t2).unwrap();

        let defs = registry.list();
        assert_eq!(defs.len(), 2);

        let mut names: Vec<_> = registry.names();
        names.sort();
        assert_eq!(names, vec!["alpha", "beta"]);
    }

    #[tokio::test]
    async fn test_registry_search() {
        let registry = Registry::new();
        registry
            .register(
                from_fn(
                    "file_read",
                    "Read a file",
                    |_ctx: Context, _: AddInput| async move { Ok(text_result("")) },
                )
                .unwrap(),
            )
            .unwrap();
        registry
            .register(
                from_fn(
                    "web_search",
                    "Search the web",
                    |_ctx: Context, _: AddInput| async move { Ok(text_result("")) },
                )
                .unwrap(),
            )
            .unwrap();

        let results = registry.search("file");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "file_read");

        let results = registry.search("search");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "web_search");
    }

    #[test]
    fn test_definition_serialization() {
        let def = Definition {
            name: "test".to_string(),
            description: "Test tool".to_string(),
            input_schema: serde_json::json!({"type": "object"}),
            output_schema: None,
            annotations: Annotations {
                title: "Test".to_string(),
                ..Default::default()
            },
            envelope: Envelope::default(),
        };

        let json = serde_json::to_value(&def).unwrap();
        assert_eq!(json["name"], "test");
        assert_eq!(json["annotations"]["title"], "Test");
        assert!(json.get("output_schema").is_none());
    }

    #[test]
    fn test_annotations_execution_hint() {
        let ann = Annotations {
            execution_hint: ExecutionHint::Ui,
            ..Default::default()
        };
        assert_eq!(ann.execution_hint, ExecutionHint::Ui);

        let default_ann = Annotations::default();
        assert_eq!(default_ann.execution_hint, ExecutionHint::Backend);
    }

    #[test]
    fn test_execution_hint_serialization() {
        let ann = Annotations {
            title: "My Tool".to_string(),
            execution_hint: ExecutionHint::Hybrid,
            ..Default::default()
        };
        let json = serde_json::to_value(&ann).unwrap();
        assert_eq!(json["execution_hint"], "hybrid");
        assert_eq!(json["title"], "My Tool");

        let ann_default = Annotations {
            title: "Other".to_string(),
            ..Default::default()
        };
        let json_default = serde_json::to_value(&ann_default).unwrap();
        assert_eq!(json_default["execution_hint"], "backend");
    }

    #[test]
    fn test_execution_hint_deserialization() {
        let json = serde_json::json!({
            "title": "T",
            "execution_hint": "backend"
        });
        let ann: Annotations = serde_json::from_value(json).unwrap();
        assert_eq!(ann.execution_hint, ExecutionHint::Backend);

        let json_missing = serde_json::json!({"title": "T"});
        let ann2: Annotations = serde_json::from_value(json_missing).unwrap();
        assert_eq!(ann2.execution_hint, ExecutionHint::Backend);
    }

    /// Minimal Callable for tests that need custom annotations.
    struct StubTool(Definition);

    #[async_trait::async_trait]
    impl Callable for StubTool {
        fn definition(&self) -> &Definition {
            &self.0
        }
        fn validate(&self, _input: &serde_json::Value) -> rskit_schema::ValidationResult {
            rskit_schema::ValidationResult {
                valid: true,
                errors: vec![],
            }
        }
        async fn call(&self, _ctx: &Context, _input: serde_json::Value) -> AppResult<ToolResult> {
            Ok(text_result("stub"))
        }
    }

    fn stub_def(name: &str, annotations: Annotations) -> Box<dyn Callable> {
        Box::new(StubTool(Definition {
            name: name.to_string(),
            description: name.to_string(),
            input_schema: serde_json::json!({"type": "object"}),
            output_schema: None,
            annotations,
            envelope: Envelope::default(),
        }))
    }

    #[tokio::test]
    async fn test_registry_filter_by_execution_hint() {
        let registry = Registry::new();

        registry
            .register(stub_def(
                "validate_form",
                Annotations {
                    execution_hint: ExecutionHint::Ui,
                    ..Default::default()
                },
            ))
            .unwrap();

        registry
            .register(stub_def(
                "run_query",
                Annotations {
                    execution_hint: ExecutionHint::Backend,
                    ..Default::default()
                },
            ))
            .unwrap();

        registry
            .register(stub_def("noop", Annotations::default()))
            .unwrap();

        let ui = registry.filter_by_execution_hint(ExecutionHint::Ui);
        assert_eq!(ui.len(), 1);
        assert_eq!(ui[0].name, "validate_form");

        let backend = registry.filter_by_execution_hint(ExecutionHint::Backend);
        let mut backend_names = backend
            .iter()
            .map(|def| def.name.as_str())
            .collect::<Vec<_>>();
        backend_names.sort_unstable();
        assert_eq!(backend_names, vec!["noop", "run_query"]);

        let hybrid = registry.filter_by_execution_hint(ExecutionHint::Hybrid);
        assert!(hybrid.is_empty());
    }

    #[test]
    fn test_context_metadata() {
        let mut ctx = Context::new();
        ctx.set("key", serde_json::json!("value"));
        assert_eq!(ctx.get("key").unwrap(), &serde_json::json!("value"));
        assert!(ctx.get("missing").is_none());
    }

    #[test]
    fn test_context_cancellation() {
        let token = tokio_util::sync::CancellationToken::new();
        let ctx = Context::with_cancellation(token.clone());
        assert!(!ctx.is_cancelled());
        token.cancel();
        assert!(ctx.is_cancelled());
    }

    #[test]
    fn test_tool_result_text() {
        let r = text_result("hello");
        assert_eq!(r.text(), "hello");
        assert!(!r.is_error);
    }

    #[test]
    fn test_tool_result_error() {
        let r = error_result("something broke");
        assert_eq!(r.text(), "something broke");
        assert!(r.is_error);
    }

    #[test]
    fn test_tool_result_json() {
        let r = json_result(&serde_json::json!({"x": 1})).unwrap();
        assert!(!r.is_error);
        assert!(r.output.is_some());
        assert_eq!(r.output.unwrap()["x"], 1);
    }

    #[test]
    fn test_tool_result_metadata() {
        let mut r = text_result("hi");
        r.set_meta("timing_ms", serde_json::json!(42));
        assert_eq!(r.metadata.get("timing_ms").unwrap(), &serde_json::json!(42));
    }
}
