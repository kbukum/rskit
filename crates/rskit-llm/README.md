# rskit-llm — LLM Provider Abstractions

LLM provider abstractions for OpenAI and Anthropic.

[![CI](https://github.com/kbukum/rskit/actions/workflows/ci.yml/badge.svg)](https://github.com/kbukum/rskit/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/rskit-llm.svg)](https://crates.io/crates/rskit-llm)
[![docs.rs](https://docs.rs/rskit-llm/badge.svg)](https://docs.rs/rskit-llm)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![MSRV: 1.85](https://img.shields.io/badge/MSRV-1.85-orange.svg)](https://blog.rust-lang.org/2025/02/20/Rust-1.85.0.html)

## Features

- `LlmProvider` trait — `complete(CompletionRequest)` and `embed(texts)`
- `OpenAiProvider` — OpenAI-compatible backend (configurable base URL)
- `AnthropicProvider` — Anthropic Claude backend
- Typed request/response: `ChatMessage`, `Role`, `CompletionRequest`, `CompletionResponse`
- `TokenUsage` tracking (input + output tokens)
- Streaming support

## Usage

```toml
[dependencies]
rskit-llm = "0.1"
```

```rust
use rskit_llm::{LlmProvider, OpenAiProvider, OpenAiConfig, CompletionRequest, ChatMessage, Role};

async fn example() {
    let provider = OpenAiProvider::new(OpenAiConfig {
        api_key: "sk-…".into(),
        base_url: "https://api.openai.com/v1".into(),
        ..Default::default()
    }).unwrap();

    let resp = provider.complete(CompletionRequest {
        model: "gpt-4o-mini".into(),
        messages: vec![ChatMessage { role: Role::User, content: "Hi".into() }],
        max_tokens: Some(64),
        temperature: Some(0.7),
        stream: false,
    }).await.unwrap();

    println!("{}", resp.content);
}
```

## See Also

[Main repository README](https://github.com/kbukum/rskit)
