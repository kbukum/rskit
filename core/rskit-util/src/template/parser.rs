use super::error::TemplateError;
use super::{Placeholder, Template, TemplatePart};

/// Parse a template string with typed placeholders.
pub(super) fn parse_template<P>(
    value: &str,
    placeholders: &[P],
) -> Result<Template<P>, TemplateError>
where
    P: Placeholder,
{
    let mut parts = Vec::new();
    let mut remaining = value;

    while let Some(start) = remaining.find('{') {
        if start > 0 {
            push_literal(&mut parts, value, &remaining[..start])?;
        }
        let after_open = &remaining[start + 1..];
        let Some(end) = after_open.find('}') else {
            return Err(TemplateError::UnclosedPlaceholder(value.to_string()));
        };
        let token = &after_open[..end];
        parts.push(TemplatePart::Placeholder(parse_placeholder(
            token,
            placeholders,
        )?));
        remaining = &after_open[end + 1..];
    }

    if !remaining.is_empty() {
        push_literal(&mut parts, value, remaining)?;
    }

    Ok(Template { parts })
}

fn push_literal<P>(
    parts: &mut Vec<TemplatePart<P>>,
    source: &str,
    literal: &str,
) -> Result<(), TemplateError> {
    if literal.contains('}') {
        return Err(TemplateError::UnmatchedClosingBrace(source.to_string()));
    }
    parts.push(TemplatePart::Literal(literal.to_string()));
    Ok(())
}

fn parse_placeholder<P>(token: &str, placeholders: &[P]) -> Result<P, TemplateError>
where
    P: Placeholder,
{
    if token.is_empty() {
        return Err(TemplateError::EmptyPlaceholder);
    }
    placeholders
        .iter()
        .copied()
        .find(|placeholder| placeholder.token() == token)
        .ok_or_else(|| TemplateError::UnknownPlaceholder(token.to_string()))
}
