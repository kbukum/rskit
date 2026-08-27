//! Token counting port and a dependency-free heuristic default.
//!
//! [`TokenCounter`] is the canonical tokenization seam for rskit: it counts the
//! number of tokens in a text string deterministically. Core ships only the
//! dependency-free [`HeuristicTokenCounter`] approximation — real tokenizers
//! (`OpenAI` BPE, `HuggingFace`) live in feature-gated `contrib/llm/*` adapters and
//! are injected explicitly, never wired at import time.

use rskit_errors::AppResult;

/// Counts the number of tokens a piece of text decomposes into.
///
/// Implementations must be deterministic: the same input always yields the same
/// count. Counting is fallible — exact tokenizers can fail to encode
/// pathological input — so implementations surface that failure as an
/// [`AppResult`] rather than swallowing it into a bogus count. This is the
/// canonical tokenization port owned by `rskit-llm`; exact model-specific
/// tokenizers are provided by `contrib/llm/*` adapters that implement this
/// trait.
pub trait TokenCounter: Send + Sync {
    /// Returns the number of tokens in `text`.
    ///
    /// An empty string yields `0`. Returns an error if the underlying tokenizer
    /// fails to encode `text`.
    fn count(&self, text: &str) -> AppResult<usize>;
}

/// Dependency-free approximate [`TokenCounter`].
///
/// Uses a coarse heuristic of roughly one token per four characters, the same
/// approximation used across rskit's chat helpers. This is an estimate, not an
/// exact tokenizer: reach for a `contrib/llm` adapter (`OpenAI` BPE, `HuggingFace`)
/// when precise, model-specific counts matter.
#[derive(Debug, Clone, Copy, Default)]
pub struct HeuristicTokenCounter;

impl TokenCounter for HeuristicTokenCounter {
    fn count(&self, text: &str) -> AppResult<usize> {
        if text.is_empty() {
            return Ok(0);
        }
        Ok(text.chars().count().div_ceil(4))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn heuristic_empty_is_zero() {
        assert_eq!(HeuristicTokenCounter.count("").unwrap(), 0);
    }

    #[test]
    fn heuristic_known_values_are_stable() {
        // chars/4 rounded up: 4 chars -> 1, 8 chars -> 2, 5 chars -> 2.
        assert_eq!(HeuristicTokenCounter.count("a").unwrap(), 1);
        assert_eq!(HeuristicTokenCounter.count("abcd").unwrap(), 1);
        assert_eq!(HeuristicTokenCounter.count("abcde").unwrap(), 2);
        assert_eq!(HeuristicTokenCounter.count("abcdefgh").unwrap(), 2);
    }

    #[test]
    fn heuristic_is_deterministic() {
        let counter = HeuristicTokenCounter;
        let text = "the quick brown fox";
        assert_eq!(counter.count(text).unwrap(), counter.count(text).unwrap());
    }

    #[test]
    fn heuristic_grows_with_length() {
        // A property of the char/4 heuristic specifically, not a contract of the
        // port: exact subword tokenizers make no such guarantee.
        let counter = HeuristicTokenCounter;
        let mut prev = 0;
        let mut text = String::new();
        for _ in 0..64 {
            text.push('x');
            let count = counter.count(&text).unwrap();
            assert!(
                count >= prev,
                "heuristic count must not decrease as text grows"
            );
            prev = count;
        }
    }

    #[test]
    fn heuristic_counts_chars_not_bytes() {
        // Four multi-byte characters (12 UTF-8 bytes) count as one token under
        // char/4, but would count as three under byte/4 — proving the heuristic
        // measures characters rather than UTF-8 bytes.
        assert_eq!(HeuristicTokenCounter.count("日本語文").unwrap(), 1);
    }

    #[test]
    fn token_counter_is_object_safe() {
        let counter: std::sync::Arc<dyn TokenCounter> = std::sync::Arc::new(HeuristicTokenCounter);
        assert_eq!(counter.count("abcd").unwrap(), 1);
    }
}
