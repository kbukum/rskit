//! Open-set, `{{var}}` brace templates with runtime-supplied variables.
//!
//! Unlike [`Template`](super::Template), which validates against a fixed,
//! typed placeholder set known at compile time, a [`DynamicTemplate`] carries
//! an open set of string-named variables resolved against a caller-supplied
//! lookup at render time. Names are matched leniently: `{{ name }}` and
//! `{{name}}` are equivalent, and any sequence that is not a well-formed
//! placeholder is preserved verbatim. Use this for prompt-style templates
//! whose variable set is data-driven rather than enumerated in code.

use std::collections::BTreeSet;

use super::error::TemplateError;

/// One parsed part of a dynamic template.
#[derive(Debug, Clone, Eq, PartialEq)]
enum Part {
    /// Literal text.
    Literal(String),
    /// A variable name (without braces).
    Variable(String),
}

/// A parsed `{{var}}` template with an open set of named variables.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct DynamicTemplate {
    parts: Vec<Part>,
}

impl DynamicTemplate {
    /// Parse a template string. Parsing is lenient and never fails: malformed
    /// `{{` / `}}` runs and invalid names are kept as literal text.
    #[must_use]
    pub fn parse(template: &str) -> Self {
        let mut parts = Vec::new();
        let mut rest = template;
        while let Some(start) = rest.find("{{") {
            let (before, after) = (&rest[..start], &rest[start + 2..]);
            if !before.is_empty() {
                parts.push(Part::Literal(before.to_owned()));
            }
            let Some(end) = after.find("}}") else {
                parts.push(Part::Literal("{{".to_owned()));
                rest = after;
                continue;
            };
            let name = after[..end].trim();
            if is_valid_name(name) {
                parts.push(Part::Variable(name.to_owned()));
            } else {
                parts.push(Part::Literal(format!("{{{{{}}}}}", &after[..end])));
            }
            rest = &after[end + 2..];
        }
        if !rest.is_empty() {
            parts.push(Part::Literal(rest.to_owned()));
        }
        Self { parts }
    }

    /// Return the sorted set of variable names referenced by the template.
    #[must_use]
    pub fn variables(&self) -> BTreeSet<String> {
        self.parts
            .iter()
            .filter_map(|part| match part {
                Part::Variable(name) => Some(name.clone()),
                Part::Literal(_) => None,
            })
            .collect()
    }

    /// Render the template, resolving each variable via `lookup`. A variable
    /// with no value yields [`TemplateError::MissingVariable`].
    pub fn render<F>(&self, mut lookup: F) -> Result<String, TemplateError>
    where
        F: FnMut(&str) -> Option<String>,
    {
        let mut out = String::new();
        for part in &self.parts {
            match part {
                Part::Literal(text) => out.push_str(text),
                Part::Variable(name) => {
                    let value =
                        lookup(name).ok_or_else(|| TemplateError::MissingVariable(name.clone()))?;
                    out.push_str(&value);
                }
            }
        }
        Ok(out)
    }
}

fn is_valid_name(name: &str) -> bool {
    !name.is_empty()
        && name
            .chars()
            .all(|c| c == '_' || c == '-' || c.is_ascii_alphanumeric())
}

#[cfg(test)]
mod tests {
    use super::{DynamicTemplate, TemplateError};

    #[test]
    fn renders_dynamic_variables_with_optional_whitespace() {
        let tpl = DynamicTemplate::parse("hi {{ name }}, {{count}} items");
        let out = tpl
            .render(|n| match n {
                "name" => Some("ada".to_owned()),
                "count" => Some("3".to_owned()),
                _ => None,
            })
            .expect("renders");
        assert_eq!(out, "hi ada, 3 items");
    }

    #[test]
    fn lists_referenced_variables() {
        let tpl = DynamicTemplate::parse("{{a}}{{ b }}{{a}}");
        let vars = tpl.variables();
        assert!(vars.contains("a") && vars.contains("b") && vars.len() == 2);
    }

    #[test]
    fn missing_variable_errors() {
        let tpl = DynamicTemplate::parse("{{x}}");
        let err = tpl.render(|_| None).expect_err("missing");
        assert_eq!(err, TemplateError::MissingVariable("x".to_owned()));
    }

    #[test]
    fn malformed_placeholders_are_literal() {
        let tpl = DynamicTemplate::parse("a {{ bad name }} b {{");
        let out = tpl.render(|_| None).expect("no valid vars");
        assert_eq!(out, "a {{ bad name }} b {{");
    }
}
