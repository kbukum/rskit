# rskit-llm — LLM provider abstractions

`rskit-llm` owns the SDK-free completion contract for chat models: requests, responses, canonical tool-use blocks, capability metadata, and the single `Provider` trait used across the Rust kit.

## Install

```toml
[dependencies]
rskit-llm = "0.1"
rskit-llm-providers = "0.1"
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

## Quick start

```rust
use futures::StreamExt;
use rskit_llm::{Provider, CompletionRequest, user};
use rskit_llm_providers::openai::{self, Config};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let provider = openai::new_adapter(&Config {
        api_key: std::env::var("OPENAI_API_KEY")?,
        base_url: "https://api.openai.com/v1".into(),
        model: "gpt-4o".into(),
        embedding_model: "text-embedding-3-small".into(),
        embedding_dimensions: 1536,
    })?;

    let request = CompletionRequest {
        model: "gpt-4o".into(),
        messages: vec![user("Summarize why explicit registration is safer.")],
        max_tokens: Some(128),
        temperature: Some(0.2),
        stream: false,
        tools: None,
        tool_choice: None,
    };

    let response = provider.complete(request.clone()).await?;
    println!("{}", response.text());

    let mut stream = provider.stream(request).await?;
    while let Some(_event) = stream.next().await {}
    Ok(())
}
```

## When to use

Use `rskit-llm` for canonical chat completions and stream events. Use `rskit-inference` for serving-runtime protocols such as Triton, vLLM, and TGI.
