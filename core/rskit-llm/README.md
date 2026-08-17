# rskit-llm — LLM provider abstractions

`rskit-llm` owns the SDK-free completion contract for chat models: requests, responses, canonical tool-use blocks, capability metadata, and the single `Provider` trait used across the Rust kit.

## Install

```toml
[dependencies]
rskit-llm = "0.2.0-alpha.4"
rskit-llm-openai = "0.2.0-alpha.5"
rskit-util = "0.2.0-alpha.6"
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

## Quick start

```rust
use futures::StreamExt;
use rskit_llm::{CompletionRequest, Registry, user};
use rskit_llm_openai::{self as openai, Config};
use rskit_util::SecretString;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut registry = Registry::new();
    openai::register(&mut registry, Config {
        api_key: SecretString::new(std::env::var("OPENAI_API_KEY")?),
        base_url: "https://api.openai.com/v1".into(),
        model: "gpt-4o".into(),
        embedding_model: "text-embedding-3-small".into(),
        embedding_dimensions: Some(1536),
    })?;
    let provider = registry.build("openai")?;

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

Use `rskit-llm` for canonical chat completions and stream events.
Use `rskit-inference` for serving-runtime protocols such as Triton, vLLM, and TGI.
