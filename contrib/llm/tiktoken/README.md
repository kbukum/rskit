# rskit-llm-tiktoken

OpenAI BPE (tiktoken) adapter implementing the `rskit_llm::TokenCounter` port.

This crate wraps [`tiktoken-rs`](https://docs.rs/tiktoken-rs) so a benchmark run, budget check, or prompt sizer can obtain exact OpenAI token counts. The BPE ranks ship inside `tiktoken-rs`, so counting is fully offline — no network access and no import-time side effects.

## Usage

```rust
use rskit_llm::TokenCounter;
use rskit_llm_tiktoken::{Encoding, TiktokenCounter, counter};

// Construct a concrete counter for an explicit encoding.
let tk = TiktokenCounter::new(Encoding::Cl100kBase)?;
assert!(tk.count("hello world")? > 0);

// Or inject one as a shared `TokenCounter`, selected by name.
let shared = counter("o200k_base")?;
# Ok::<(), rskit_errors::AppError>(())
```

## Supported encodings

| Name | Model family |
|---|---|
| `o200k_base` | GPT-4o |
| `cl100k_base` | GPT-4, GPT-3.5-turbo |
| `p50k_base` | Codex, `text-davinci` |
| `r50k_base` (`gpt2`) | GPT-3 |

Unknown names are rejected with an `InvalidInput` error rather than silently falling back to a default.

## Feature flag

Enabled through the facade with the `llm-tiktoken` feature (off by default).
