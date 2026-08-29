//! Supported `OpenAI` BPE encodings selectable by configuration.

use rskit_errors::{AppError, AppResult, ErrorCode};

/// An `OpenAI` BPE encoding supported by the tiktoken adapter.
///
/// Selecting an encoding is explicit: callers name the encoding (typically
/// derived from the target model) rather than relying on any implicit default.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[non_exhaustive]
pub enum Encoding {
    /// `o200k_base` — GPT-4o family.
    O200kBase,
    /// `cl100k_base` — GPT-4 and GPT-3.5-turbo family.
    Cl100kBase,
    /// `p50k_base` — Codex and older `text-davinci` models.
    P50kBase,
    /// `r50k_base` (a.k.a. `gpt2`) — original GPT-3 models.
    R50kBase,
}

impl Encoding {
    /// Parses an encoding from its canonical tiktoken name.
    ///
    /// Returns an [`ErrorCode::InvalidInput`] error for unknown names rather
    /// than falling back to a default, so misconfiguration surfaces loudly.
    pub fn from_name(name: &str) -> AppResult<Self> {
        match name {
            "o200k_base" => Ok(Self::O200kBase),
            "cl100k_base" => Ok(Self::Cl100kBase),
            "p50k_base" => Ok(Self::P50kBase),
            "r50k_base" | "gpt2" => Ok(Self::R50kBase),
            other => Err(AppError::new(
                ErrorCode::InvalidInput,
                format!("unknown tiktoken encoding: {other}"),
            )),
        }
    }

    /// Returns the canonical tiktoken name for this encoding.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::O200kBase => "o200k_base",
            Self::Cl100kBase => "cl100k_base",
            Self::P50kBase => "p50k_base",
            Self::R50kBase => "r50k_base",
        }
    }
}
