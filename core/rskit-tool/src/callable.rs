//! Callable trait — type-erased tool interface.

use async_trait::async_trait;
use rskit_errors::AppResult;
use rskit_validation::ValidationResult;

use crate::context::Context;
use crate::definition::Definition;
use crate::result::ToolResult;

/// Type-erased tool interface for heterogeneous registries.
#[async_trait]
pub trait Callable: Send + Sync {
    /// Return the tool's metadata.
    fn definition(&self) -> &Definition;

    /// Validate input against the tool's input schema.
    fn validate(&self, input: &serde_json::Value) -> ValidationResult;

    /// Execute the tool with a context and JSON input.
    async fn call(&self, ctx: &Context, input: serde_json::Value) -> AppResult<ToolResult>;
}
