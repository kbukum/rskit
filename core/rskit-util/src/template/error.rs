use std::fmt;

/// Errors returned by template parsing and rendering.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum TemplateError {
    /// The template has an unclosed placeholder (missing closing brace).
    UnclosedPlaceholder(String),
    /// The template has an unmatched closing brace.
    UnmatchedClosingBrace(String),
    /// A placeholder inside the template has an empty name.
    EmptyPlaceholder,
    /// A placeholder inside the template is not recognized.
    UnknownPlaceholder(String),
    /// A custom error returned by the renderer callback.
    Render(String),
}

impl fmt::Display for TemplateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::UnclosedPlaceholder(s) => write!(f, "unclosed placeholder in '{s}'"),
            Self::UnmatchedClosingBrace(s) => {
                write!(f, "unmatched closing placeholder brace in '{s}'")
            }
            Self::EmptyPlaceholder => write!(f, "placeholder cannot be empty"),
            Self::UnknownPlaceholder(p) => write!(f, "unknown placeholder '{p}'"),
            Self::Render(err) => write!(f, "template render failed: {err}"),
        }
    }
}

impl std::error::Error for TemplateError {}
