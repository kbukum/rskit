//! Lightweight template engine featuring brace-delimited placeholders.

use std::fmt;

mod dynamic;
mod error;
mod parser;
mod renderer;

pub use dynamic::DynamicTemplate;
pub use error::TemplateError;

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
    pub fn parse(value: &str, placeholders: &[P]) -> Result<Self, TemplateError> {
        parser::parse_template(value, placeholders)
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
}

#[cfg(test)]
mod tests {
    use super::*;

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
        assert_eq!(
            error,
            TemplateError::UnknownPlaceholder("project.root".to_string())
        );
    }

    #[test]
    fn rejects_unclosed_placeholders() {
        let error = Template::parse("cargo {name", TOKENS).expect_err("unclosed fails");
        assert_eq!(
            error,
            TemplateError::UnclosedPlaceholder("cargo {name".to_string())
        );
    }

    #[test]
    fn rejects_empty_placeholders() {
        let error = Template::parse("{}", TOKENS).expect_err("empty fails");
        assert_eq!(error, TemplateError::EmptyPlaceholder);
    }

    #[test]
    fn rejects_unmatched_closing_braces() {
        let error =
            Template::parse("cargo } {name}", TOKENS).expect_err("closing brace should fail");
        assert_eq!(
            error,
            TemplateError::UnmatchedClosingBrace("cargo } {name}".to_string())
        );
    }

    #[test]
    fn renders_with_callback() {
        let template = Template::parse("{name}:{args}", TOKENS).expect("template parses");

        let rendered = template
            .render_with(|placeholder| match placeholder {
                Token::Name => Ok::<_, &'static str>("build".to_string()),
                Token::Args => Ok::<_, &'static str>("--all".to_string()),
            })
            .expect("template renders");

        assert_eq!(rendered, "build:--all");
    }
}
