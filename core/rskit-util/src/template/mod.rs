//! Lightweight template engine featuring brace-delimited placeholders.

mod dynamic;
mod error;
mod parser;
mod renderer;
mod typed;

pub use dynamic::DynamicTemplate;
pub use error::TemplateError;
pub use typed::{Placeholder, Template, TemplatePart};
