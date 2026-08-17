# rskit-llm-anthropic — Anthropic provider adapter

`rskit-llm-anthropic` registers an Anthropic Claude chat provider for the canonical `rskit-llm` registry. The adapter keeps Anthropic-specific API details behind the shared `rskit-llm::Provider` contract.

## Install

```toml
[dependencies]
rskit-llm = "0.2.0-alpha.4"
rskit-llm-anthropic = "0.2.0-alpha.5"
rskit-util = "0.2.0-alpha.6"
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

## Quick start

```rust
use rskit_llm::{CompletionRequest, Registry, user};
use rskit_llm_anthropic::{self as anthropic, Config};
use rskit_util::SecretString;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut registry = Registry::new();
    anthropic::register(&mut registry, Config {
        api_key: SecretString::new(std::env::var("ANTHROPIC_API_KEY")?),
        base_url: "https://api.anthropic.com".into(),
        model: "claude-sonnet-4-20250514".into(),
        api_version: "2023-06-01".into(),
    })?;

    let provider = registry.build("anthropic")?;
    let response = provider
        .complete(CompletionRequest {
            model: "claude-sonnet-4-20250514".into(),
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

Use this adapter when an application should talk to Anthropic through the provider-agnostic `rskit-llm` API. Keep Anthropic API keys in secret configuration fields and pass them as `SecretString`.
