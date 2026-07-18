//! JSON Schema generation and validation from Rust types.
//!
//! Thin wrapper around [`schemars`] providing a consistent API for generating
//! JSON Schema documents from any type implementing `JsonSchema`, plus a
//! runtime validator for checking JSON values against schemas.

#![warn(missing_docs)]

pub use schemars::JsonSchema;

mod document;
mod generation;
mod json;
mod limits;
mod validation;

pub use document::SchemaDocument;
pub use generation::{Options, generate, generate_document, generate_with, generate_with_options};
pub use json::Json;
pub use limits::ValidationLimits;
#[cfg(feature = "validation")]
pub use validation::{
    CompiledSchema, compile, compile_with_options, validate, validate_structured_output,
    validate_with_options,
};
pub use validation::{ValidationError, ValidationOptions, ValidationResult};

#[cfg(all(test, feature = "validation"))]
mod tests;
