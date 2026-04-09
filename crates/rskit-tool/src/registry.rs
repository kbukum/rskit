//! Concurrent-safe tool registry.

use parking_lot::RwLock;
use rskit_errors::{AppError, AppResult, ErrorCode};
use std::collections::HashMap;
use std::sync::Arc;

use crate::callable::Callable;
use crate::context::Context;
use crate::definition::Definition;
use crate::result::ToolResult;

/// Thread-safe registry of callable tools.
pub struct Registry {
    tools: RwLock<HashMap<String, Arc<dyn Callable>>>,
}

impl Registry {
    /// Create a new empty registry.
    pub fn new() -> Self {
        Self {
            tools: RwLock::new(HashMap::new()),
        }
    }

    /// Register a tool. Returns error on duplicate names.
    pub fn register(&self, tool: Box<dyn Callable>) -> AppResult<()> {
        let name = tool.definition().name.clone();
        let mut tools = self.tools.write();
        if tools.contains_key(&name) {
            return Err(AppError::new(
                ErrorCode::AlreadyExists,
                format!("tool already registered: {name:?}"),
            ));
        }
        tools.insert(name, Arc::from(tool));
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
    pub async fn call(
        &self,
        name: &str,
        ctx: &Context,
        input: serde_json::Value,
    ) -> AppResult<ToolResult> {
        let tool = self.get(name).ok_or_else(|| {
            AppError::new(ErrorCode::NotFound, format!("tool not found: {name:?}"))
        })?;
        tool.call(ctx, input).await
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

    /// Call multiple tools concurrently.
    ///
    /// Read-only tools are executed concurrently via `tokio::join!`.
    /// Non-read-only tools are executed serially to avoid side-effect conflicts.
    pub async fn call_batch(
        &self,
        calls: Vec<(&str, serde_json::Value)>,
        ctx: &Context,
    ) -> Vec<AppResult<ToolResult>> {
        let mut read_only_futs = Vec::new();
        let mut serial_calls = Vec::new();

        for (name, input) in &calls {
            if let Some(tool) = self.get(name) {
                if tool.definition().read_only {
                    read_only_futs.push((tool, input.clone()));
                } else {
                    serial_calls.push((name.to_string(), tool, input.clone()));
                }
            } else {
                serial_calls.push((
                    name.to_string(),
                    Arc::from(Box::new(NotFoundTool(name.to_string())) as Box<dyn Callable>),
                    input.clone(),
                ));
            }
        }

        let mut results = Vec::new();

        // Execute read-only tools concurrently
        if !read_only_futs.is_empty() {
            let handles: Vec<_> = read_only_futs
                .into_iter()
                .map(|(tool, input)| {
                    let ctx = ctx.clone();
                    let tool = tool.clone();
                    tokio::spawn(async move { tool.call(&ctx, input).await })
                })
                .collect();

            for handle in handles {
                match handle.await {
                    Ok(r) => results.push(r),
                    Err(e) => results.push(Err(AppError::new(
                        ErrorCode::Internal,
                        format!("task join error: {e}"),
                    ))),
                }
            }
        }

        // Execute non-read-only tools serially
        for (name, tool, input) in serial_calls {
            if tool.definition().name == name || tool.definition().name.is_empty() {
                // NotFoundTool check
                if tool.definition().name.is_empty() {
                    results.push(Err(AppError::new(
                        ErrorCode::NotFound,
                        format!("tool not found: {name:?}"),
                    )));
                    continue;
                }
            }
            results.push(tool.call(ctx, input).await);
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

impl Default for Registry {
    fn default() -> Self {
        Self::new()
    }
}

/// Placeholder for not-found tools in call_batch.
struct NotFoundTool(String);

#[async_trait::async_trait]
impl Callable for NotFoundTool {
    fn definition(&self) -> &Definition {
        // This is only used for the not-found case; we use a static-like approach
        // by leaking a definition. Since this is transient, we construct it inline.
        // A better approach: we handle not-found above. This is a fallback.
        static EMPTY_DEF: std::sync::LazyLock<Definition> =
            std::sync::LazyLock::new(|| Definition {
                name: String::new(),
                description: String::new(),
                input_schema: serde_json::Value::Null,
                output_schema: None,
                annotations: None,
                read_only: false,
                destructive: false,
                max_result_size: 0,
                timeout_secs: 0.0,
            });
        &EMPTY_DEF
    }

    fn validate(&self, _input: &serde_json::Value) -> rskit_schema::ValidationResult {
        rskit_schema::ValidationResult {
            valid: false,
            errors: vec![rskit_schema::ValidationError {
                path: String::new(),
                message: format!("tool not found: {:?}", self.0),
            }],
        }
    }

    async fn call(&self, _ctx: &Context, _input: serde_json::Value) -> AppResult<ToolResult> {
        Err(AppError::new(
            ErrorCode::NotFound,
            format!("tool not found: {:?}", self.0),
        ))
    }
}
