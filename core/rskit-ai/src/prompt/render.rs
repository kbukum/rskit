//! Prompt rendering helpers.

use std::collections::BTreeSet;

use rskit_util::template::{DynamicTemplate, TemplateError};

use crate::chat::{Message, SystemMessage};

use super::template::{PromptError, PromptTemplate, RenderContext};

/// Render a template string using `{{var}}` placeholders. Handles optional whitespace: `{{ var }}`
/// and `{{var}}` are equivalent.
pub fn render(template: &str, context: &RenderContext) -> Result<String, PromptError> {
    DynamicTemplate::parse(template)
        .render(|name| {
            context.get(name).map(|value| {
                value
                    .as_str()
                    .map_or_else(|| value.to_string(), ToString::to_string)
            })
        })
        .map_err(|error| match error {
            TemplateError::MissingVariable(name) => PromptError::MissingVariable(name),
            other => PromptError::MissingVariable(other.to_string()),
        })
}

/// Convert rendered prompts into AI messages.
pub trait RenderToMessage {
    /// Render into a system message.
    fn render_to_message(&self, context: &RenderContext) -> Result<Message, PromptError>;
}

impl RenderToMessage for PromptTemplate {
    fn render_to_message(&self, context: &RenderContext) -> Result<Message, PromptError> {
        Ok(Message::System(SystemMessage {
            content: self.render(context)?,
        }))
    }
}

pub(crate) fn placeholders(template: &str) -> BTreeSet<String> {
    DynamicTemplate::parse(template).variables()
}
