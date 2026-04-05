# rskit-inference — LLM Inference Provider

Inference provider abstractions for LLM chat completions.

[![CI](https://github.com/kbukum/rskit/actions/workflows/ci.yml/badge.svg)](https://github.com/kbukum/rskit/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/rskit-inference.svg)](https://crates.io/crates/rskit-inference)
[![docs.rs](https://docs.rs/rskit-inference/badge.svg)](https://docs.rs/rskit-inference)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![MSRV: 1.85](https://img.shields.io/badge/MSRV-1.85-orange.svg)](https://blog.rust-lang.org/2025/02/20/Rust-1.85.0.html)

## Features

- `InferenceProvider` trait — `complete(CompletionRequest) → AppResult<CompletionResponse>`
- `OpenAiInferenceProvider` — works with OpenAI, llama.cpp, vLLM, Ollama
- Simple `Message` type with role and content strings
- Token usage tracking via `CompletionResponse.usage_tokens`
- Configurable endpoint, API key, and model

## Usage

```toml
[dependencies]
rskit-inference = "0.1"
```

```rust
use rskit_inference::{
    InferenceProvider, OpenAiInferenceProvider, OpenAiInferenceConfig,
    CompletionRequest, Message,
};

async fn example() {
    let provider = OpenAiInferenceProvider::new(OpenAiInferenceConfig {
        endpoint: "http://localhost:8000".into(),
        api_key: "".into(),
        model: "mistral-7b".into(),
    });

    let resp = provider.complete(CompletionRequest {
        messages: vec![Message { role: "user".into(), content: "Hello".into() }],
        max_tokens: Some(128),
        temperature: Some(0.7),
    }).await.unwrap();

    println!("{} (tokens: {})", resp.content, resp.usage_tokens);
}
```

## See Also

[Main repository README](https://github.com/kbukum/rskit)
