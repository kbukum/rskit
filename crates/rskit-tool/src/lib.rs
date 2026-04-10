//! Tool definition, auto-wiring, registry and middleware for agentic systems.
//!
//! Provides a type-safe framework for defining tools that can be used in
//! agentic systems, LLM function calling, or MCP servers.

mod callable;
pub mod context;
mod definition;
mod from_fn;
mod middleware;
mod middleware_metrics;
mod middleware_retry;
pub mod registry;
pub mod result;

pub use callable::Callable;
pub use context::Context;
pub use definition::{Annotations, Definition};
pub use from_fn::{from_fn, from_fn_simple};
pub use middleware::{
    Middleware, chain, with_logging, with_result_limit, with_timeout, with_validation,
};
pub use middleware_metrics::{InMemoryMetrics, MetricRecord, MetricsCollector, with_metrics};
pub use middleware_retry::{RetryConfig, RetryPredicate, with_retry};
pub use registry::Registry;
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
        );

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
        });

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
        });

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
        });

        let ctx = Context::new();
        let result = tool.call(&ctx, serde_json::json!({"x": 1})).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_validate() {
        let tool = from_fn("add", "Add", |_ctx: Context, _input: AddInput| async move {
            Ok(text_result("0"))
        });

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
        );

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
        });
        let t2 = from_fn("dup", "Second", |_ctx: Context, _: AddInput| async move {
            Ok(text_result("2"))
        });

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
        });
        let t2 = from_fn("beta", "B tool", |_ctx: Context, _: AddInput| async move {
            Ok(text_result("b"))
        });

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
            .register(from_fn(
                "file_read",
                "Read a file",
                |_ctx: Context, _: AddInput| async move { Ok(text_result("")) },
            ))
            .unwrap();
        registry
            .register(from_fn(
                "web_search",
                "Search the web",
                |_ctx: Context, _: AddInput| async move { Ok(text_result("")) },
            ))
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
            annotations: Some(Annotations {
                title: Some("Test".to_string()),
                read_only_hint: Some(true),
                ..Default::default()
            }),
            read_only: true,
            destructive: false,
            max_result_size: 0,
            timeout_secs: 0.0,
        };

        let json = serde_json::to_value(&def).unwrap();
        assert_eq!(json["name"], "test");
        assert_eq!(json["annotations"]["read_only_hint"], true);
        assert!(json.get("output_schema").is_none());
    }

    #[test]
    fn test_annotations_execution_hint() {
        let ann = Annotations {
            execution_hint: Some("ui".to_string()),
            ..Default::default()
        };
        assert_eq!(ann.execution_hint.as_deref(), Some("ui"));

        let default_ann = Annotations::default();
        assert!(default_ann.execution_hint.is_none());
    }

    #[test]
    fn test_execution_hint_serialization() {
        // execution_hint present — included in JSON
        let ann = Annotations {
            title: Some("My Tool".to_string()),
            execution_hint: Some("hybrid".to_string()),
            ..Default::default()
        };
        let json = serde_json::to_value(&ann).unwrap();
        assert_eq!(json["execution_hint"], "hybrid");
        assert_eq!(json["title"], "My Tool");

        // execution_hint None — omitted from JSON (skip_serializing_if)
        let ann_none = Annotations {
            title: Some("Other".to_string()),
            ..Default::default()
        };
        let json_none = serde_json::to_value(&ann_none).unwrap();
        assert!(json_none.get("execution_hint").is_none());
    }

    #[test]
    fn test_execution_hint_deserialization() {
        let json = serde_json::json!({
            "title": "T",
            "execution_hint": "backend"
        });
        let ann: Annotations = serde_json::from_value(json).unwrap();
        assert_eq!(ann.execution_hint.as_deref(), Some("backend"));

        // Missing field deserializes as None
        let json_missing = serde_json::json!({"title": "T"});
        let ann2: Annotations = serde_json::from_value(json_missing).unwrap();
        assert!(ann2.execution_hint.is_none());
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
        async fn call(
            &self,
            _ctx: &Context,
            _input: serde_json::Value,
        ) -> AppResult<ToolResult> {
            Ok(text_result("stub"))
        }
    }

    fn stub_def(name: &str, annotations: Option<Annotations>) -> Box<dyn Callable> {
        Box::new(StubTool(Definition {
            name: name.to_string(),
            description: name.to_string(),
            input_schema: serde_json::json!({"type": "object"}),
            output_schema: None,
            annotations,
            read_only: false,
            destructive: false,
            max_result_size: 0,
            timeout_secs: 0.0,
        }))
    }

    #[tokio::test]
    async fn test_registry_filter_by_execution_hint() {
        let registry = Registry::new();

        registry
            .register(stub_def(
                "validate_form",
                Some(Annotations {
                    execution_hint: Some("ui".to_string()),
                    ..Default::default()
                }),
            ))
            .unwrap();

        registry
            .register(stub_def(
                "run_query",
                Some(Annotations {
                    execution_hint: Some("backend".to_string()),
                    ..Default::default()
                }),
            ))
            .unwrap();

        // Tool with no annotations at all
        registry.register(stub_def("noop", None)).unwrap();

        let ui = registry.filter_by_execution_hint("ui");
        assert_eq!(ui.len(), 1);
        assert_eq!(ui[0].name, "validate_form");

        let backend = registry.filter_by_execution_hint("backend");
        assert_eq!(backend.len(), 1);
        assert_eq!(backend[0].name, "run_query");

        let hybrid = registry.filter_by_execution_hint("hybrid");
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
