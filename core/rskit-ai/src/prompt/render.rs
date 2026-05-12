//! Prompt rendering helpers.

use std::collections::BTreeSet;

use crate::chat::{Message, SystemMessage};

use super::template::{PromptError, PromptTemplate, RenderContext};

/// Render a template string using `{{var}}` placeholders.
/// Handles optional whitespace: `{{ var }}` and `{{var}}` are equivalent.
pub fn render(template: &str, context: &RenderContext) -> Result<String, PromptError> {
    let mut rendered = String::with_capacity(template.len());
    let mut rest = template;
    while let Some(start) = rest.find("{{") {
        rendered.push_str(&rest[..start]);
        rest = &rest[start + 2..];
        let Some(end) = rest.find("}}") else {
            rendered.push_str("{{");
            break;
        };
        let name = rest[..end].trim();
        if name.is_empty()
            || !name
                .chars()
                .all(|c| c == '_' || c == '-' || c.is_ascii_alphanumeric())
        {
            rendered.push_str("{{");
            rendered.push_str(&rest[..end + 2]);
            rest = &rest[end + 2..];
            continue;
        }
        let value = context
            .get(name)
            .ok_or_else(|| PromptError::MissingVariable(name.to_owned()))?;
        let replacement = value
            .as_str()
            .map_or_else(|| value.to_string(), ToString::to_string);
        rendered.push_str(&replacement);
        rest = &rest[end + 2..];
    }
    rendered.push_str(rest);
    Ok(rendered)
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
    let mut names = BTreeSet::new();
    let mut rest = template;
    while let Some(start) = rest.find("{{") {
        rest = &rest[start + 2..];
        let Some(end) = rest.find("}}") else { break };
        let name = rest[..end].trim();
        if !name.is_empty()
            && name
                .chars()
                .all(|c| c == '_' || c == '-' || c.is_ascii_alphanumeric())
        {
            names.insert(name.to_owned());
        }
        rest = &rest[end + 2..];
    }
    names
}
