//! Auto-wiring: create tools from async functions.

use async_trait::async_trait;
use rskit_errors::{AppError, AppResult, ErrorCode};
use rskit_validation::{CompiledSchema, ValidationResult};
use schemars::JsonSchema;
use serde::{Serialize, de::DeserializeOwned};

use crate::callable::Callable;
use crate::context::Context;
use crate::definition::Definition;
use crate::result::ToolResult;

/// Create a tool from an async handler function.
///
/// Generates JSON Schemas from the input and output type parameters
/// using schemars, and wraps the handler in a type-erased `Callable`.
///
/// The handler receives a `Context` reference and deserialized input,
/// returning a `ToolResult`.
///
/// # Example
///
/// ```rust,ignore
/// use rskit_tool::from_fn;
///
/// let tool = from_fn("search", "Search the web", |_ctx, input: SearchInput| async move {
///     Ok(rskit_tool::result::text_result(&format!("found: {}", input.query)))
/// })?;
/// ```
pub fn from_fn<I, F, Fut>(name: &str, description: &str, handler: F) -> AppResult<Box<dyn Callable>>
where
    I: DeserializeOwned + JsonSchema + Send + 'static,
    F: Fn(Context, I) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = AppResult<ToolResult>> + Send + 'static,
{
    let input_schema = rskit_schema::generate::<I>()?;
    let input_validator = rskit_validation::compile(&input_schema)?;

    let def = Definition {
        name: name.to_string(),
        description: description.to_string(),
        input_schema,
        output_schema: None,
        annotations: crate::Annotations::default(),
        envelope: crate::Envelope::default(),
    };

    Ok(Box::new(FnTool {
        definition: def,
        input_validator,
        handler,
        _phantom: std::marker::PhantomData,
    }))
}

struct FnTool<F, I> {
    definition: Definition,
    input_validator: CompiledSchema,
    handler: F,
    _phantom: std::marker::PhantomData<fn(I)>,
}

#[async_trait]
impl<I, F, Fut> Callable for FnTool<F, I>
where
    I: DeserializeOwned + Send + 'static,
    F: Fn(Context, I) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = AppResult<ToolResult>> + Send + 'static,
{
    fn definition(&self) -> &Definition {
        &self.definition
    }

    fn validate(&self, input: &serde_json::Value) -> ValidationResult {
        self.input_validator.validate(input)
    }

    async fn call(&self, ctx: &Context, input: serde_json::Value) -> AppResult<ToolResult> {
        let typed_input: I = serde_json::from_value(input).map_err(|e| {
            AppError::new(ErrorCode::InvalidInput, format!("invalid tool input: {e}"))
        })?;

        (self.handler)(ctx.clone(), typed_input).await
    }
}

/// Convenience: create a tool from a simpler handler that returns a Serialize type.
///
/// The handler takes only the input (no Context) and returns `AppResult<O>`.
/// The output is automatically serialized to JSON.
pub fn from_fn_simple<I, O, F, Fut>(
    name: &str,
    description: &str,
    handler: F,
) -> AppResult<Box<dyn Callable>>
where
    I: DeserializeOwned + JsonSchema + Send + 'static,
    O: Serialize + Send + 'static,
    F: Fn(I) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = AppResult<O>> + Send + 'static,
{
    let input_schema = rskit_schema::generate::<I>()?;
    let input_validator = rskit_validation::compile(&input_schema)?;

    let def = Definition {
        name: name.to_string(),
        description: description.to_string(),
        input_schema,
        output_schema: None,
        annotations: crate::Annotations::default(),
        envelope: crate::Envelope::default(),
    };

    Ok(Box::new(SimpleFnTool {
        definition: def,
        input_validator,
        handler,
        _phantom: std::marker::PhantomData,
    }))
}

struct SimpleFnTool<F, I, O> {
    definition: Definition,
    input_validator: CompiledSchema,
    handler: F,
    _phantom: std::marker::PhantomData<fn(I) -> O>,
}

#[async_trait]
impl<I, O, F, Fut> Callable for SimpleFnTool<F, I, O>
where
    I: DeserializeOwned + Send + 'static,
    O: Serialize + Send + 'static,
    F: Fn(I) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = AppResult<O>> + Send + 'static,
{
    fn definition(&self) -> &Definition {
        &self.definition
    }

    fn validate(&self, input: &serde_json::Value) -> ValidationResult {
        self.input_validator.validate(input)
    }

    async fn call(&self, _ctx: &Context, input: serde_json::Value) -> AppResult<ToolResult> {
        let typed_input: I = serde_json::from_value(input).map_err(|e| {
            AppError::new(ErrorCode::InvalidInput, format!("invalid tool input: {e}"))
        })?;

        let output = (self.handler)(typed_input).await?;

        let json = serde_json::to_value(&output).map_err(|e| {
            AppError::new(
                ErrorCode::Internal,
                format!("failed to serialize tool output: {e}"),
            )
        })?;
        let content = serde_json::to_string(&output).unwrap_or_default();

        Ok(ToolResult {
            output: Some(json),
            content,
            is_error: false,
            metadata: std::collections::HashMap::new(),
        })
    }
}
