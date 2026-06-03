//! Typed placeholder-template parsing and rendering primitives.

use std::fmt;

use rskit_errors::{AppError, AppResult};

/// A placeholder token that can be parsed from a template.
pub trait Placeholder: Copy + Eq + fmt::Display {
    /// Return the user-facing token name, without braces.
    fn token(self) -> &'static str;
}

/// One parsed template part.
#[derive(Debug, Clone, Eq, PartialEq)]
pub enum TemplatePart<P> {
    /// Literal text.
    Literal(String),
    /// Placeholder token.
    Placeholder(P),
}

/// Parsed template string with typed placeholders.
#[derive(Debug, Clone, Eq, PartialEq)]
pub struct Template<P> {
    parts: Vec<TemplatePart<P>>,
}

impl<P> Template<P>
where
    P: Placeholder,
{
    /// Parse a template string and reject unknown placeholders.
    pub fn parse(value: &str, placeholders: &[P]) -> AppResult<Self> {
        let mut parts = Vec::new();
        let mut remaining = value;

        while let Some(start) = remaining.find('{') {
            if start > 0 {
                push_literal(&mut parts, value, &remaining[..start])?;
            }
            let after_open = &remaining[start + 1..];
            let Some(end) = after_open.find('}') else {
                return Err(AppError::invalid_input(
                    "template",
                    format!("unclosed placeholder in '{value}'"),
                ));
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

        Ok(Self { parts })
    }

    /// Return parsed template parts.
    #[must_use]
    pub fn parts(&self) -> &[TemplatePart<P>] {
        &self.parts
    }

    /// Return true when the template contains the placeholder.
    #[must_use]
    pub fn contains(&self, placeholder: P) -> bool {
        self.parts
            .iter()
            .any(|part| matches!(part, TemplatePart::Placeholder(found) if *found == placeholder))
    }

    /// Render the template using a placeholder renderer callback.
    pub fn render_with<F>(&self, mut render: F) -> AppResult<String>
    where
        F: FnMut(P) -> AppResult<String>,
    {
        let mut rendered = String::new();
        for part in &self.parts {
            match part {
                TemplatePart::Literal(value) => rendered.push_str(value),
                TemplatePart::Placeholder(placeholder) => {
                    rendered.push_str(&render(*placeholder)?);
                }
            }
        }
        Ok(rendered)
    }
}

fn push_literal<P>(parts: &mut Vec<TemplatePart<P>>, source: &str, literal: &str) -> AppResult<()> {
    if literal.contains('}') {
        return Err(AppError::invalid_input(
            "template",
            format!("unmatched closing placeholder brace in '{source}'"),
        ));
    }
    parts.push(TemplatePart::Literal(literal.to_string()));
    Ok(())
}

fn parse_placeholder<P>(token: &str, placeholders: &[P]) -> AppResult<P>
where
    P: Placeholder,
{
    if token.is_empty() {
        return Err(AppError::invalid_input(
            "template",
            "placeholder cannot be empty",
        ));
    }
    placeholders
        .iter()
        .copied()
        .find(|placeholder| placeholder.token() == token)
        .ok_or_else(|| {
            AppError::invalid_input("template", format!("unknown placeholder '{token}'"))
        })
}

#[cfg(test)]
mod tests {
    use std::fmt;

    use super::{Placeholder, Template, TemplatePart};

    #[derive(Debug, Clone, Copy, Eq, PartialEq)]
    enum Token {
        Name,
        Args,
    }

    impl Placeholder for Token {
        fn token(self) -> &'static str {
            match self {
                Self::Name => "name",
                Self::Args => "args",
            }
        }
    }

    impl fmt::Display for Token {
        fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
            formatter.write_str(self.token())
        }
    }

    const TOKENS: &[Token] = &[Token::Name, Token::Args];

    #[test]
    fn parses_known_placeholders() {
        let template = Template::parse("cargo {name} {args}", TOKENS).expect("template parses");

        assert!(template.contains(Token::Name));
        assert!(template.contains(Token::Args));
        assert_eq!(
            template.parts(),
            [
                TemplatePart::Literal("cargo ".to_string()),
                TemplatePart::Placeholder(Token::Name),
                TemplatePart::Literal(" ".to_string()),
                TemplatePart::Placeholder(Token::Args),
            ]
        );
    }

    #[test]
    fn rejects_unknown_placeholders() {
        let error = Template::parse("{project.root}", TOKENS).expect_err("unknown fails");

        assert!(error.message().contains("unknown placeholder"));
    }

    #[test]
    fn rejects_unclosed_placeholders() {
        let error = Template::parse("cargo {name", TOKENS).expect_err("unclosed fails");

        assert!(error.message().contains("unclosed placeholder"));
    }

    #[test]
    fn rejects_empty_placeholders() {
        let error = Template::parse("{}", TOKENS).expect_err("empty fails");

        assert!(error.message().contains("placeholder cannot be empty"));
    }

    #[test]
    fn rejects_unmatched_closing_braces() {
        let error =
            Template::parse("cargo } {name}", TOKENS).expect_err("closing brace should fail");

        assert!(error.message().contains("unmatched closing"));
    }

    #[test]
    fn renders_with_callback() {
        let template = Template::parse("{name}:{args}", TOKENS).expect("template parses");

        let rendered = template
            .render_with(|placeholder| match placeholder {
                Token::Name => Ok("build".to_string()),
                Token::Args => Ok("--all".to_string()),
            })
            .expect("template renders");

        assert_eq!(rendered, "build:--all");
    }
}
