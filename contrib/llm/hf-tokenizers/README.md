# rskit-llm-hf-tokenizers

HuggingFace tokenizers adapter implementing the `rskit_llm::TokenCounter` port.

This crate wraps the [`tokenizers`](https://docs.rs/tokenizers) crate so a benchmark run, budget check, or prompt sizer can count tokens with any HuggingFace `tokenizer.json`. The tokenizer is loaded explicitly from a caller-supplied path or an in-memory definition — nothing is downloaded and there are no import-time side effects.

## Usage

```rust,no_run
use rskit_llm::TokenCounter;
use rskit_llm_hf_tokenizers::{HfTokenCounter, counter};

// Load a concrete counter from a tokenizer.json path.
let hf = HfTokenCounter::from_file("tokenizer.json")?;
assert!(hf.count("hello world")? > 0);

// Or inject one as a shared `TokenCounter`.
let shared = counter("tokenizer.json")?;
# Ok::<(), rskit_errors::AppError>(())
```

A tokenizer can also be built from an in-memory definition with `HfTokenCounter::from_json`. Missing paths and malformed definitions are rejected with an `InvalidInput` error.

## Feature flag

Enabled through the facade with the `llm-hf-tokenizers` feature (off by default).
