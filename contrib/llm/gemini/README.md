# rskit-llm-gemini — Gemini provider adapter

`rskit-llm-gemini` registers a Google Gemini chat provider for the canonical `rskit-llm` registry. It translates the shared completion contract into the Gemini Generative Language API shape.

## Install

```toml
[dependencies]
rskit-llm = "0.2.0-alpha.1"
rskit-llm-gemini = "0.2.0-alpha.2"
rskit-util = "0.2.0-alpha.3"
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

## Quick start

```rust
use rskit_llm::{CompletionRequest, Registry, user};
use rskit_llm_gemini::{self as gemini, Config};
use rskit_util::SecretString;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut registry = Registry::new();
    gemini::register(&mut registry, Config {
        api_key: SecretString::new(std::env::var("GEMINI_API_KEY")?),
        base_url: "https://generativelanguage.googleapis.com".into(),
        model: "gemini-2.5-flash".into(),
    })?;

    let provider = registry.build("gemini")?;
    let response = provider
        .complete(CompletionRequest {
            model: "gemini-2.5-flash".into(),
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

Use this adapter when an application should talk to Gemini through the provider-agnostic `rskit-llm` API. Use `rskit-ai` vocabulary and `rskit-observability` GenAI attributes for cross-provider telemetry.
