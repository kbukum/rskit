//! [`TokenCounter`] implementation backed by a `HuggingFace` tokenizer.

use rskit_errors::{AppError, AppResult, ErrorCode};
use rskit_llm::TokenCounter;
use std::io::Read;
use std::path::Path;
use std::sync::Arc;
use tokenizers::Tokenizer;

/// Default maximum size, in bytes, of a `tokenizer.json` definition loaded from
/// disk.
///
/// Real tokenizer definitions are at most a few tens of megabytes; the cap
/// keeps a malicious or accidentally-huge file from exhausting process memory
/// before it is parsed. Callers with unusual needs can override it via
/// [`HfTokenCounter::from_file_with_max_bytes`].
pub const DEFAULT_MAX_DEFINITION_BYTES: u64 = 64 * 1024 * 1024;

/// Counts tokens using a `HuggingFace` `tokenizer.json` via the `tokenizers` crate.
///
/// The tokenizer is loaded explicitly from a caller-supplied vocab/config path
/// or an in-memory definition; nothing is downloaded and there are no
/// import-time side effects.
pub struct HfTokenCounter {
    tokenizer: Tokenizer,
    id: String,
}

impl HfTokenCounter {
    /// Loads a counter from a `HuggingFace` `tokenizer.json` file path.
    ///
    /// The file is read under [`DEFAULT_MAX_DEFINITION_BYTES`]; a larger file is
    /// rejected with `InvalidInput` before it is parsed.
    pub fn from_file(path: impl AsRef<Path>) -> AppResult<Self> {
        Self::from_file_with_max_bytes(path, DEFAULT_MAX_DEFINITION_BYTES)
    }

    /// Loads a counter from a `tokenizer.json` file path under a caller-supplied
    /// byte budget.
    ///
    /// The read is bounded to `max_bytes`, so a definition that exceeds the
    /// budget — even one that grows after the initial size probe — is rejected
    /// with `InvalidInput` rather than being copied into memory.
    pub fn from_file_with_max_bytes(path: impl AsRef<Path>, max_bytes: u64) -> AppResult<Self> {
        let path = path.as_ref();
        let json = read_bounded(path, max_bytes)?;
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
        let id = format!("hf-tokenizers:{}", fingerprint(json.as_bytes()));
        Ok(Self { tokenizer, id })
    }
}

/// Computes a deterministic, dependency-free fingerprint of a tokenizer
/// definition using the 64-bit FNV-1a hash.
///
/// The fingerprint is stable across processes and platforms for identical
/// bytes, so two counters loaded from the same `tokenizer.json` share an
/// identity while different definitions do not. It is an identity fingerprint,
/// not a cryptographic digest.
fn fingerprint(bytes: &[u8]) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        hash ^= u64::from(b);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

/// Reads a UTF-8 tokenizer definition from `path`, rejecting anything larger
/// than `max_bytes`.
///
/// The read is capped at `max_bytes + 1` so a file that grows between the
/// metadata probe and the read (or one whose reported length lies) is still
/// caught rather than streamed unbounded into memory.
fn read_bounded(path: &Path, max_bytes: u64) -> AppResult<String> {
    let file = std::fs::File::open(path).map_err(|e| {
        AppError::new(
            ErrorCode::InvalidInput,
            format!("failed to open tokenizer file {}", path.display()),
        )
        .with_cause(e)
    })?;
    let mut buf = Vec::new();
    let read = file
        .take(max_bytes.saturating_add(1))
        .read_to_end(&mut buf)
        .map_err(|e| {
            AppError::new(
                ErrorCode::InvalidInput,
                format!("failed to read tokenizer file {}", path.display()),
            )
            .with_cause(e)
        })?;
    if read as u64 > max_bytes {
        return Err(AppError::new(
            ErrorCode::InvalidInput,
            format!(
                "tokenizer file {} exceeds the {max_bytes}-byte limit",
                path.display()
            ),
        ));
    }
    String::from_utf8(buf).map_err(|e| {
        AppError::new(
            ErrorCode::InvalidInput,
            format!("tokenizer file {} is not valid UTF-8", path.display()),
        )
        .with_cause(e)
    })
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

    fn id(&self) -> String {
        self.id.clone()
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
    fn valid_json_that_is_not_a_tokenizer_is_rejected() {
        // Parses as JSON (so the dropout probe passes) but is not a valid
        // tokenizer definition, exercising the `Tokenizer::from_bytes` failure
        // path rather than the earlier serde parse.
        let err = HfTokenCounter::from_json(r#"{"not":"a tokenizer"}"#)
            .err()
            .expect("non-tokenizer json must error");
        assert_eq!(err.code(), ErrorCode::InvalidInput);
    }

    #[test]
    fn non_utf8_file_is_rejected() {
        use std::io::Write;
        let mut file = tempfile_json();
        // Invalid UTF-8 byte sequence exercises the `from_utf8` failure path.
        file.write_all(&[0xff, 0xfe, 0x00]).unwrap();
        file.flush().unwrap();
        let err = HfTokenCounter::from_file(file.path())
            .err()
            .expect("non-utf8 file must error");
        assert_eq!(err.code(), ErrorCode::InvalidInput);
    }

    #[test]
    fn unreadable_path_is_rejected() {
        // A directory opens successfully but fails to read, exercising the
        // read (not open) failure path in `read_bounded`.
        let dir = tempfile::tempdir().expect("temp dir");
        let err = HfTokenCounter::from_file(dir.path())
            .err()
            .expect("directory path must error");
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

    #[test]
    fn oversized_file_is_rejected_before_parsing() {
        use std::io::Write;
        let mut file = tempfile_json();
        file.write_all(FIXTURE.as_bytes()).unwrap();
        file.flush().unwrap();
        // A budget smaller than the fixture must be rejected as InvalidInput
        // rather than parsed.
        let max = (FIXTURE.len() as u64) - 1;
        let err = HfTokenCounter::from_file_with_max_bytes(file.path(), max)
            .err()
            .expect("oversized file must error");
        assert_eq!(err.code(), ErrorCode::InvalidInput);
    }

    #[test]
    fn file_within_budget_loads() {
        use std::io::Write;
        let mut file = tempfile_json();
        file.write_all(FIXTURE.as_bytes()).unwrap();
        file.flush().unwrap();
        let counter =
            HfTokenCounter::from_file_with_max_bytes(file.path(), FIXTURE.len() as u64).unwrap();
        assert_eq!(counter.count("hello world").unwrap(), 2);
    }
}
