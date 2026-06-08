# rskit-llm-ollama — Ollama provider adapter

`rskit-llm-ollama` registers a local or remote Ollama provider for the canonical `rskit-llm` registry. Ollama exposes an OpenAI-compatible chat-completions endpoint at `<base_url>/v1/chat/completions`.

## Install

```toml
[dependencies]
rskit-llm = "0.1.0-alpha.1"
rskit-llm-ollama = "0.1.0-alpha.1"
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

## Quick start

```rust
use rskit_llm::{CompletionRequest, Registry, user};
use rskit_llm_ollama::{self as ollama, Config};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut registry = Registry::new();
    ollama::register(&mut registry, Config {
        base_url: "http://localhost:11434".into(),
        model: "llama3.2".into(),
        api_key: None,
    })?;

    let provider = registry.build("ollama")?;
    let response = provider
        .complete(CompletionRequest {
            model: "llama3.2".into(),
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

Use this adapter for local development, private model hosting, or remote Ollama deployments that should still use the shared `rskit-llm::Provider` contract.
