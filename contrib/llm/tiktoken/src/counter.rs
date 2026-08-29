//! [`TokenCounter`] implementation backed by `OpenAI`'s BPE (tiktoken).

use crate::config::Encoding;
use rskit_errors::{AppError, AppResult, ErrorCode};
use rskit_llm::TokenCounter;
use std::sync::Arc;
use tiktoken_rs::CoreBPE;

/// Counts tokens using an `OpenAI` BPE encoding via `tiktoken-rs`.
///
/// The encoding is chosen explicitly at construction; the BPE ranks are bundled
/// in `tiktoken-rs`, so counting is fully offline with no network access or
/// import-time side effects.
pub struct TiktokenCounter {
    encoding: Encoding,
    bpe: CoreBPE,
}

impl TiktokenCounter {
    /// Builds a counter for the given [`Encoding`].
    pub fn new(encoding: Encoding) -> AppResult<Self> {
        let bpe = load_encoding(encoding)?;
        Ok(Self { encoding, bpe })
    }

    /// Builds a counter from a tiktoken encoding name (e.g. `"cl100k_base"`).
    pub fn from_name(name: &str) -> AppResult<Self> {
        Self::new(Encoding::from_name(name)?)
    }

    /// Returns the encoding this counter uses.
    #[must_use]
    pub const fn encoding(&self) -> Encoding {
        self.encoding
    }
}

/// Builds a [`TiktokenCounter`] for `name` as a shared [`TokenCounter`].
///
/// This is the explicit registration seam: the caller injects the returned
/// counter wherever a [`TokenCounter`] is required.
pub fn counter(name: &str) -> AppResult<Arc<dyn TokenCounter>> {
    Ok(Arc::new(TiktokenCounter::from_name(name)?))
}

fn load_encoding(encoding: Encoding) -> AppResult<CoreBPE> {
    let result = match encoding {
        Encoding::O200kBase => tiktoken_rs::o200k_base(),
        Encoding::Cl100kBase => tiktoken_rs::cl100k_base(),
        Encoding::P50kBase => tiktoken_rs::p50k_base(),
        Encoding::R50kBase => tiktoken_rs::r50k_base(),
    };
    result.map_err(|e| {
        AppError::new(
            ErrorCode::Internal,
            format!("failed to load tiktoken encoding {}", encoding.name()),
        )
        .with_boxed_cause(e.into())
    })
}

impl TokenCounter for TiktokenCounter {
    fn count(&self, text: &str) -> AppResult<usize> {
        // `encode_ordinary` is infallible over any UTF-8 input, but the port
        // contract is fallible, so the count is returned as a success.
        Ok(self.bpe.encode_ordinary(text).len())
    }

    fn id(&self) -> String {
        format!("tiktoken:{}", self.encoding.name())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_text_is_zero() {
        let counter = TiktokenCounter::new(Encoding::Cl100kBase).unwrap();
        assert_eq!(counter.count("").unwrap(), 0);
    }

    #[test]
    fn counts_known_text_deterministically() {
        let counter = TiktokenCounter::new(Encoding::Cl100kBase).unwrap();
        // "hello world" is a stable two-token sequence under cl100k_base.
        let first = counter.count("hello world").unwrap();
        assert_eq!(first, 2);
        assert_eq!(counter.count("hello world").unwrap(), first);
    }

    #[test]
    fn from_name_selects_encoding() {
        let counter = TiktokenCounter::from_name("o200k_base").unwrap();
        assert_eq!(counter.encoding(), Encoding::O200kBase);
    }

    #[test]
    fn all_encodings_load_and_count() {
        for encoding in [
            Encoding::O200kBase,
            Encoding::Cl100kBase,
            Encoding::P50kBase,
            Encoding::R50kBase,
        ] {
            let counter = TiktokenCounter::new(encoding).unwrap();
            assert_eq!(counter.encoding(), encoding);
            assert!(counter.count("hello world").unwrap() > 0);
        }
    }

    #[test]
    fn every_name_round_trips_to_its_encoding() {
        for (name, expected) in [
            ("o200k_base", Encoding::O200kBase),
            ("cl100k_base", Encoding::Cl100kBase),
            ("p50k_base", Encoding::P50kBase),
            ("r50k_base", Encoding::R50kBase),
        ] {
            let encoding = Encoding::from_name(name).unwrap();
            assert_eq!(encoding, expected);
            assert_eq!(encoding.name(), name);
        }
    }

    #[test]
    fn gpt2_alias_maps_to_r50k_base() {
        let counter = TiktokenCounter::from_name("gpt2").unwrap();
        assert_eq!(counter.encoding(), Encoding::R50kBase);
        // The canonical name for the alias is r50k_base.
        assert_eq!(counter.encoding().name(), "r50k_base");
    }

    #[test]
    fn unknown_encoding_is_rejected() {
        let err = TiktokenCounter::from_name("does_not_exist")
            .err()
            .expect("unknown encoding must error");
        assert_eq!(err.code(), ErrorCode::InvalidInput);
    }

    #[test]
    fn counter_returns_shared_token_counter() {
        let shared: Arc<dyn TokenCounter> = counter("cl100k_base").unwrap();
        assert_eq!(shared.count("hello world").unwrap(), 2);
    }
}
