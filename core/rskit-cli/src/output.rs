//! Structured terminal output — tables and key-value displays.

use std::fmt;

use rskit_errors::{AppError, ErrorCode};
use serde::Serialize;

/// Machine-readable output format for CLI renderers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum OutputFormat {
    /// Human-readable terminal text.
    #[default]
    Text,
    /// JSON object or array.
    Json,
    /// YAML document.
    Yaml,
}

/// Process exit code convention shared by rskit CLIs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(i32)]
#[non_exhaustive]
pub enum ExitCode {
    /// Successful command.
    Success = 0,
    /// Invalid command input or configuration.
    Usage = 2,
    /// Authentication or authorization failure.
    Permission = 3,
    /// Requested resource was not found.
    NotFound = 4,
    /// Conflict with current state.
    Conflict = 5,
    /// Remote dependency or service failure.
    Unavailable = 69,
    /// Command was rate limited.
    RateLimited = 75,
    /// Command timed out.
    Timeout = 124,
    /// Command was cancelled.
    Cancelled = 130,
    /// Unclassified failure.
    Failure = 1,
}

impl ExitCode {
    /// Return this exit code as an integer suitable for `std::process::exit`.
    #[must_use]
    pub const fn as_i32(self) -> i32 {
        self as i32
    }
}

impl From<ErrorCode> for ExitCode {
    fn from(code: ErrorCode) -> Self {
        match code {
            ErrorCode::InvalidInput | ErrorCode::InvalidFormat | ErrorCode::MissingField => {
                Self::Usage
            }
            ErrorCode::Unauthorized
            | ErrorCode::Forbidden
            | ErrorCode::TokenExpired
            | ErrorCode::InvalidToken => Self::Permission,
            ErrorCode::NotFound => Self::NotFound,
            ErrorCode::Conflict | ErrorCode::AlreadyExists => Self::Conflict,
            ErrorCode::ServiceUnavailable
            | ErrorCode::ConnectionFailed
            | ErrorCode::ExternalService => Self::Unavailable,
            ErrorCode::RateLimited => Self::RateLimited,
            ErrorCode::Timeout => Self::Timeout,
            ErrorCode::Cancelled => Self::Cancelled,
            _ => Self::Failure,
        }
    }
}

/// Renders [`AppError`] values consistently for command-line applications.
pub struct ErrorRenderer {
    format: OutputFormat,
}

impl ErrorRenderer {
    /// Create a renderer for the requested output format.
    #[must_use]
    pub const fn new(format: OutputFormat) -> Self {
        Self { format }
    }

    /// Render an error and return the matching CLI exit code.
    #[must_use]
    pub fn render(&self, error: &AppError) -> (String, ExitCode) {
        let exit_code = ExitCode::from(error.code());
        let rendered = match self.format {
            OutputFormat::Text => format!("error[{}]: {}", error.code(), error.message()),
            OutputFormat::Json => serde_json::to_string(&ErrorEnvelope::new(error, exit_code))
                .unwrap_or_else(|_| fallback_json(error, exit_code)),
            OutputFormat::Yaml => serde_norway::to_string(&ErrorEnvelope::new(error, exit_code))
                .unwrap_or_else(|_| fallback_yaml(error, exit_code)),
        };
        (rendered, exit_code)
    }
}

impl Default for ErrorRenderer {
    fn default() -> Self {
        Self::new(OutputFormat::Text)
    }
}

#[derive(Serialize)]
struct ErrorEnvelope<'a> {
    code: ErrorCode,
    message: &'a str,
    retryable: bool,
    http_status: u16,
    exit_code: i32,
    #[serde(skip_serializing_if = "serde_json::Map::is_empty")]
    details: serde_json::Map<String, serde_json::Value>,
}

impl<'a> ErrorEnvelope<'a> {
    fn new(error: &'a AppError, exit_code: ExitCode) -> Self {
        Self {
            code: error.code(),
            message: error.message(),
            retryable: error.is_retryable(),
            http_status: error.http_status().as_u16(),
            exit_code: exit_code.as_i32(),
            details: error.details().clone().into_iter().collect(),
        }
    }
}

fn fallback_json(error: &AppError, exit_code: ExitCode) -> String {
    format!(
        r#"{{"code":"{}","message":{},"exit_code":{}}}"#,
        error.code(),
        serde_json::Value::String(error.message().to_string()),
        exit_code.as_i32()
    )
}

fn fallback_yaml(error: &AppError, exit_code: ExitCode) -> String {
    format!(
        "code: {}\nmessage: {}\nexit_code: {}\n",
        error.code(),
        serde_json::Value::String(error.message().to_string()),
        exit_code.as_i32()
    )
}

/// A formatted table for terminal output.
pub struct OutputTable {
    title: Option<String>,
    columns: Vec<String>,
    rows: Vec<Vec<String>>,
}

impl OutputTable {
    /// Create a table with the given column headings.
    #[must_use]
    pub fn new(columns: Vec<impl Into<String>>) -> Self {
        Self {
            title: None,
            columns: columns.into_iter().map(Into::into).collect(),
            rows: Vec::new(),
        }
    }

    /// Set a human-readable table title.
    #[must_use]
    pub fn with_title(mut self, title: impl Into<String>) -> Self {
        self.title = Some(title.into());
        self
    }

    pub fn add_row(&mut self, row: Vec<impl Into<String>>) {
        self.rows.push(row.into_iter().map(Into::into).collect());
    }
}

impl fmt::Display for OutputTable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut widths: Vec<usize> = self.columns.iter().map(|c| c.len()).collect();
        for row in &self.rows {
            for (i, cell) in row.iter().enumerate() {
                if i < widths.len() {
                    widths[i] = widths[i].max(cell.len());
                }
            }
        }

        if let Some(title) = &self.title {
            writeln!(f, "\n{title}")?;
        }

        let separator: String = widths
            .iter()
            .map(|w| "─".repeat(w + 2))
            .collect::<Vec<_>>()
            .join("┬");
        writeln!(f, "┌{separator}┐")?;

        let header: String = self
            .columns
            .iter()
            .enumerate()
            .map(|(i, c)| format!(" {:width$} ", c, width = widths[i]))
            .collect::<Vec<_>>()
            .join("│");
        writeln!(f, "│{header}│")?;

        let separator: String = widths
            .iter()
            .map(|w| "─".repeat(w + 2))
            .collect::<Vec<_>>()
            .join("┼");
        writeln!(f, "├{separator}┤")?;

        for row in &self.rows {
            let cells: String = row
                .iter()
                .enumerate()
                .map(|(i, c)| {
                    let w = widths.get(i).copied().unwrap_or(0);
                    format!(" {c:w$} ")
                })
                .collect::<Vec<_>>()
                .join("│");
            writeln!(f, "│{cells}│")?;
        }

        let separator: String = widths
            .iter()
            .map(|w| "─".repeat(w + 2))
            .collect::<Vec<_>>()
            .join("┴");
        write!(f, "└{separator}┘")?;

        Ok(())
    }
}

/// Key-value display for headers/summaries.
pub struct OutputKV {
    pairs: Vec<(String, String)>,
}

impl OutputKV {
    /// Create an empty key-value output block.
    #[must_use]
    pub fn new() -> Self {
        Self { pairs: Vec::new() }
    }

    pub fn add(&mut self, key: impl Into<String>, value: impl Into<String>) -> &mut Self {
        self.pairs.push((key.into(), value.into()));
        self
    }
}

impl Default for OutputKV {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Display for OutputKV {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let max_key = self.pairs.iter().map(|(k, _)| k.len()).max().unwrap_or(0);
        for (key, value) in &self.pairs {
            writeln!(f, "  {key:>max_key$}:  {value}")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn output_table_renders() {
        let mut table = OutputTable::new(vec!["Name", "Count"]);
        table.add_row(vec!["real", "500"]);
        table.add_row(vec!["ai", "500"]);
        let output = table.to_string();
        assert!(output.contains("Name"));
        assert!(output.contains("500"));
    }

    #[test]
    fn output_kv_renders() {
        let mut kv = OutputKV::new();
        kv.add("Output", "/tmp/dataset");
        kv.add("Preset", "image");
        let output = kv.to_string();
        assert!(output.contains("Output"));
        assert!(output.contains("/tmp/dataset"));
    }

    #[test]
    fn error_renderer_uses_same_exit_code_across_formats() {
        let err = AppError::not_found("repo", Some("missing"));
        for format in [OutputFormat::Text, OutputFormat::Json, OutputFormat::Yaml] {
            let (rendered, code) = ErrorRenderer::new(format).render(&err);
            assert_eq!(code, ExitCode::NotFound);
            assert!(rendered.contains("not found"));
        }
    }
}
