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
        let path = path.as_ref();
        let json = std::fs::read_to_string(path).map_err(|e| {
            AppError::new(
                ErrorCode::InvalidInput,
                format!("failed to read tokenizer file {}", path.display()),
            )
            .with_cause(e)
        })?;
        Self::from_json(&json)
    }

    /// Loads a counter from an in-memory `tokenizer.json` definition.
    ///
    /// A definition that enables nonzero BPE dropout is rejected: dropout makes
    /// `encode` stochastic, which would violate the deterministic
    /// [`TokenCounter`] contract.
    pub fn from_json(json: &str) -> AppResult<Self> {
        reject_nonzero_bpe_dropout(json)?;
        let tokenizer = Tokenizer::from_bytes(json.as_bytes()).map_err(|e| {
            AppError::new(
                ErrorCode::InvalidInput,
                "failed to parse tokenizer definition".to_string(),
            )
            .with_boxed_cause(e)
        })?;
        Ok(Self { tokenizer })
    }
}

/// Rejects a definition whose BPE model enables nonzero dropout.
///
/// BPE dropout randomizes merge application, so repeated `encode` calls become
/// stochastic and token counts stop being deterministic. The `tokenizers` API
/// does not expose the dropout parameter after construction, so the raw
/// definition is inspected instead.
fn reject_nonzero_bpe_dropout(json: &str) -> AppResult<()> {
    let value: serde_json::Value = serde_json::from_str(json).map_err(|e| {
        AppError::new(
            ErrorCode::InvalidInput,
            "failed to parse tokenizer definition".to_string(),
        )
        .with_cause(e)
    })?;
    let dropout = value
        .get("model")
        .and_then(|model| model.get("dropout"))
        .and_then(serde_json::Value::as_f64);
    if let Some(rate) = dropout
        && rate > 0.0
    {
        return Err(AppError::new(
            ErrorCode::InvalidInput,
            format!(
                "tokenizer enables nonzero BPE dropout ({rate}); token counts would be nondeterministic"
            ),
        ));
    }
    Ok(())
}

/// Loads an [`HfTokenCounter`] from a file path as a shared [`TokenCounter`].
///
/// This is the explicit registration seam: the caller injects the returned
/// counter wherever a [`TokenCounter`] is required.
pub fn counter(path: impl AsRef<Path>) -> AppResult<Arc<dyn TokenCounter>> {
    Ok(Arc::new(HfTokenCounter::from_file(path)?))
}

impl TokenCounter for HfTokenCounter {
    fn count(&self, text: &str) -> AppResult<usize> {
        if text.is_empty() {
            return Ok(0);
        }
        let encoding = self.tokenizer.encode(text, false).map_err(|e| {
            AppError::new(
                ErrorCode::Internal,
                "failed to encode text for token counting".to_string(),
            )
            .with_boxed_cause(e)
        })?;
        Ok(encoding.get_ids().len())
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
        assert_eq!(fixture_counter().count("").unwrap(), 0);
    }

    #[test]
    fn counts_known_words_deterministically() {
        let counter = fixture_counter();
        // Three whitespace-delimited tokens present in the fixture vocab.
        assert_eq!(counter.count("hello world foo").unwrap(), 3);
        assert_eq!(counter.count("hello world foo").unwrap(), 3);
    }

    #[test]
    fn unknown_words_map_to_unk_token() {
        let counter = fixture_counter();
        // Two words, both counted (unknown one maps to the [UNK] token).
        assert_eq!(counter.count("hello zzz").unwrap(), 2);
    }

    #[test]
    fn nonzero_bpe_dropout_is_rejected() {
        // A BPE model with dropout > 0 encodes stochastically, so it must be
        // rejected at construction to keep counts deterministic.
        const DROPOUT: &str = r#"{"model":{"type":"BPE","dropout":0.1,"vocab":{},"merges":[]}}"#;
        let err = HfTokenCounter::from_json(DROPOUT)
            .err()
            .expect("nonzero BPE dropout must be rejected");
        assert_eq!(err.code(), ErrorCode::InvalidInput);
    }

    #[test]
    fn zero_bpe_dropout_is_accepted() {
        // Explicit zero dropout is deterministic and must load.
        const ZERO_DROPOUT: &str = r#"{"version":"1.0","model":{"type":"BPE","dropout":0.0,"vocab":{"a":0,"b":1,"ab":2},"merges":["a b"]}}"#;
        assert!(HfTokenCounter::from_json(ZERO_DROPOUT).is_ok());
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
        assert_eq!(shared.count("hello world").unwrap(), 2);
    }

    fn tempfile_json() -> tempfile::NamedTempFile {
        tempfile::Builder::new()
            .suffix(".json")
            .tempfile()
            .expect("temp file")
    }
}
