# rskit-llm-openai — OpenAI provider adapter

`rskit-llm-openai` registers an OpenAI-compatible chat provider for the canonical `rskit-llm` registry. It uses `rskit-httpclient` for outbound transport and redacts API keys through `rskit-util::SecretString`.

## Install

```toml
[dependencies]
rskit-llm = "0.2.0-alpha.2"
rskit-llm-openai = "0.2.0-alpha.3"
rskit-util = "0.2.0-alpha.4"
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

## Quick start

```rust
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
    let response = provider
        .complete(CompletionRequest {
            model: "gpt-4o".into(),
            messages: vec![user("Summarize explicit provider registration.")],
            max_tokens: Some(128),
            temperature: Some(0.2),
            stream: false,
            tools: None,
            tool_choice: None,
        })
        .await?;

    println!("{}", response.text());
    Ok(())
}
```

## When to use

Use this adapter when you want the canonical `rskit-llm::Provider` API backed by OpenAI or an OpenAI-compatible endpoint. Use `rskit-llm` directly for provider-agnostic contracts and `rskit-embedding` plus `embedding_provider` when you need embeddings.
