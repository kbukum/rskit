//! [`TokenCounter`] implementation backed by a `HuggingFace` tokenizer.

use rskit_errors::{AppError, AppResult, ErrorCode};
use rskit_llm::TokenCounter;
use std::path::Path;
use std::sync::Arc;
use tokenizers::Tokenizer;

/// Counts tokens using a `HuggingFace` `tokenizer.json` via the `tokenizers` crate.
///
/// The tokenizer is loaded explicitly from a caller-supplied vocab/config path
/// or an in-memory definition; nothing is downloaded and there are no
/// import-time side effects.
pub struct HfTokenCounter {
    tokenizer: Tokenizer,
}

impl HfTokenCounter {
    /// Loads a counter from a `HuggingFace` `tokenizer.json` file path.
    pub fn from_file(path: impl AsRef<Path>) -> AppResult<Self> {
        let tokenizer = Tokenizer::from_file(path.as_ref()).map_err(|e| {
            AppError::new(
                ErrorCode::InvalidInput,
                format!(
                    "failed to load tokenizer from {}: {e}",
                    path.as_ref().display()
                ),
            )
        })?;
        Ok(Self { tokenizer })
    }

    /// Loads a counter from an in-memory `tokenizer.json` definition.
    pub fn from_json(json: &str) -> AppResult<Self> {
        let tokenizer = Tokenizer::from_bytes(json.as_bytes()).map_err(|e| {
            AppError::new(
                ErrorCode::InvalidInput,
                format!("failed to parse tokenizer definition: {e}"),
            )
        })?;
        Ok(Self { tokenizer })
    }
}

/// Loads an [`HfTokenCounter`] from a file path as a shared [`TokenCounter`].
///
/// This is the explicit registration seam: the caller injects the returned
/// counter wherever a [`TokenCounter`] is required.
pub fn counter(path: impl AsRef<Path>) -> AppResult<Arc<dyn TokenCounter>> {
    Ok(Arc::new(HfTokenCounter::from_file(path)?))
}

impl TokenCounter for HfTokenCounter {
    fn count(&self, text: &str) -> usize {
        if text.is_empty() {
            return 0;
        }
        // Encoding cannot fail for a loaded tokenizer over plain text; on the
        // unexpected error path we report zero rather than panicking, keeping
        // the counter total on the runtime path.
        self.tokenizer
            .encode(text, false)
            .map_or(0, |encoding| encoding.get_ids().len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FIXTURE: &str = include_str!("../tests/fixtures/tokenizer.json");

    fn fixture_counter() -> HfTokenCounter {
        HfTokenCounter::from_json(FIXTURE).expect("fixture tokenizer loads")
    }

    #[test]
    fn empty_text_is_zero() {
        assert_eq!(fixture_counter().count(""), 0);
    }

    #[test]
    fn counts_known_words_deterministically() {
        let counter = fixture_counter();
        // Three whitespace-delimited tokens present in the fixture vocab.
        assert_eq!(counter.count("hello world foo"), 3);
        assert_eq!(counter.count("hello world foo"), 3);
    }

    #[test]
    fn unknown_words_map_to_unk_token() {
        let counter = fixture_counter();
        // Two words, both counted (unknown one maps to the [UNK] token).
        assert_eq!(counter.count("hello zzz"), 2);
    }

    #[test]
    fn count_is_monotonic_with_length() {
        let counter = fixture_counter();
        assert!(counter.count("hello world foo") >= counter.count("hello"));
    }

    #[test]
    fn missing_file_is_rejected() {
        let err = HfTokenCounter::from_file("/no/such/tokenizer.json")
            .err()
            .expect("missing file must error");
        assert_eq!(err.code(), ErrorCode::InvalidInput);
    }

    #[test]
    fn malformed_json_is_rejected() {
        let err = HfTokenCounter::from_json("{ not valid")
            .err()
            .expect("malformed json must error");
        assert_eq!(err.code(), ErrorCode::InvalidInput);
    }

    #[test]
    fn counter_returns_shared_token_counter() {
        use std::io::Write;
        let mut file = tempfile_json();
        file.write_all(FIXTURE.as_bytes()).unwrap();
        let shared: Arc<dyn TokenCounter> = counter(file.path()).unwrap();
        assert_eq!(shared.count("hello world"), 2);
    }

    fn tempfile_json() -> tempfile::NamedTempFile {
        tempfile::Builder::new()
            .suffix(".json")
            .tempfile()
            .expect("temp file")
    }
}
