//! Composable prompt building with template interpolation.
//!
//! [`PromptTemplate`] wraps a template string and substitutes `{{variable}}`
//! placeholders.  [`PromptBuilder`] provides a fluent API for assembling
//! multi-section prompts.

use std::collections::HashMap;

use rskit_errors::{AppError, ErrorCode};

// ── PromptTemplate ──────────────────────────────────────────────────────────

/// A named template string with `{{variable}}` interpolation.
#[derive(Debug, Clone)]
pub struct PromptTemplate {
    pub name: String,
    template: String,
}

impl PromptTemplate {
    /// Create a new prompt template.
    pub fn new(name: &str, template: &str) -> Self {
        Self {
            name: name.to_string(),
            template: template.to_string(),
        }
    }

    /// Render the template by replacing every `{{key}}` with the corresponding
    /// value from `data`.  Returns an error if any placeholder has no matching
    /// key.
    pub fn render(&self, data: &HashMap<String, String>) -> Result<String, AppError> {
        let mut result = self.template.clone();
        let mut start = 0;

        while let Some(open) = result[start..].find("{{") {
            let open = start + open;
            let Some(close) = result[open..].find("}}") else {
                break;
            };
            let close = open + close;
            let key = result[open + 2..close].trim();

            let value = data.get(key).ok_or_else(|| {
                AppError::new(
                    ErrorCode::InvalidInput,
                    format!(
                        "prompt template '{}': missing variable '{}'",
                        self.name, key
                    ),
                )
            })?;

            result.replace_range(open..close + 2, value);
            start = open + value.len();
        }

        Ok(result)
    }
}

// ── PromptBuilder ───────────────────────────────────────────────────────────

/// Fluent builder for composing multi-section prompts.
#[derive(Debug, Clone)]
pub struct PromptBuilder {
    sections: Vec<(String, String)>,
    separator: String,
}

impl Default for PromptBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl PromptBuilder {
    /// Create a new empty builder with a default double-newline separator.
    pub fn new() -> Self {
        Self {
            sections: Vec::new(),
            separator: "\n\n".to_string(),
        }
    }

    /// Append a named section.
    pub fn section(mut self, name: &str, content: &str) -> Self {
        self.sections.push((name.to_string(), content.to_string()));
        self
    }

    /// Append a named section only when `condition` is true.
    pub fn section_if(self, condition: bool, name: &str, content: &str) -> Self {
        if condition {
            self.section(name, content)
        } else {
            self
        }
    }

    /// Override the separator placed between sections.
    pub fn separator(mut self, sep: &str) -> Self {
        self.separator = sep.to_string();
        self
    }

    /// Join all sections with the configured separator.
    pub fn build(&self) -> Result<String, AppError> {
        if self.sections.is_empty() {
            return Err(AppError::new(
                ErrorCode::InvalidInput,
                "prompt builder has no sections",
            ));
        }

        let parts: Vec<&str> = self.sections.iter().map(|(_, c)| c.as_str()).collect();
        Ok(parts.join(&self.separator))
    }
}

// ── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_render_simple() {
        let tpl = PromptTemplate::new("greet", "Hello, {{name}}!");
        let mut data = HashMap::new();
        data.insert("name".to_string(), "World".to_string());
        assert_eq!(tpl.render(&data).unwrap(), "Hello, World!");
    }

    #[test]
    fn test_render_multiple_vars() {
        let tpl = PromptTemplate::new("intro", "I am {{name}}, age {{age}}.");
        let mut data = HashMap::new();
        data.insert("name".to_string(), "Alice".to_string());
        data.insert("age".to_string(), "30".to_string());
        assert_eq!(tpl.render(&data).unwrap(), "I am Alice, age 30.");
    }

    #[test]
    fn test_render_missing_variable() {
        let tpl = PromptTemplate::new("greet", "Hello, {{name}}!");
        let data = HashMap::new();
        let err = tpl.render(&data).unwrap_err();
        assert!(err.message.contains("missing variable 'name'"));
    }

    #[test]
    fn test_render_no_placeholders() {
        let tpl = PromptTemplate::new("static", "No variables here.");
        let data = HashMap::new();
        assert_eq!(tpl.render(&data).unwrap(), "No variables here.");
    }

    #[test]
    fn test_render_trimmed_key() {
        let tpl = PromptTemplate::new("space", "Hello, {{ name }}!");
        let mut data = HashMap::new();
        data.insert("name".to_string(), "Bob".to_string());
        assert_eq!(tpl.render(&data).unwrap(), "Hello, Bob!");
    }

    #[test]
    fn test_render_repeated_variable() {
        let tpl = PromptTemplate::new("repeat", "{{x}} and {{x}}");
        let mut data = HashMap::new();
        data.insert("x".to_string(), "hi".to_string());
        assert_eq!(tpl.render(&data).unwrap(), "hi and hi");
    }

    #[test]
    fn test_builder_basic() {
        let prompt = PromptBuilder::new()
            .section("role", "You are a helpful assistant.")
            .section("task", "Answer questions accurately.")
            .build()
            .unwrap();
        assert_eq!(
            prompt,
            "You are a helpful assistant.\n\nAnswer questions accurately."
        );
    }

    #[test]
    fn test_builder_section_if() {
        let prompt = PromptBuilder::new()
            .section("role", "Assistant")
            .section_if(false, "tools", "You have tools.")
            .section_if(true, "task", "Do work.")
            .build()
            .unwrap();
        assert_eq!(prompt, "Assistant\n\nDo work.");
    }

    #[test]
    fn test_builder_custom_separator() {
        let prompt = PromptBuilder::new()
            .separator("\n---\n")
            .section("a", "Section A")
            .section("b", "Section B")
            .build()
            .unwrap();
        assert_eq!(prompt, "Section A\n---\nSection B");
    }

    #[test]
    fn test_builder_empty_error() {
        let result = PromptBuilder::new().build();
        assert!(result.is_err());
    }
}
