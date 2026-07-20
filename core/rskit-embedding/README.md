# rskit-embedding — canonical embedding abstraction

`rskit-embedding` owns the SDK-free embedding contract plus deterministic test adapter. Provider backends live in focused adapter crates and register explicitly during composition.

## Install

```toml
[dependencies]
rskit-embedding = "0.2.0-alpha.2"
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

## Quick start

```rust
use rskit_ai::{Capabilities, Model, Provider as ModelProvider};
use rskit_embedding::{EmbedInput, EmbedRequest, Provider};
use rskit_embedding::InMemoryProvider;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let provider = InMemoryProvider::new(8);

    let response = provider
        .embed(EmbedRequest {
            model: Model {
                name: "inmem-embedding".into(),
                provider: ModelProvider::Custom("memory".into()),
                version: None,
                capabilities: Capabilities::default(),
            },
            inputs: vec![EmbedInput::Text(
                "native provider shapes keep pipelines composable".into(),
            )],
            options: rskit_embedding::EmbeddingOptions::default(),
        })
        .await?;

    println!("{}", response.embeddings[0].dimensions);
    Ok(())
}
```

## When to use

Use `rskit-embedding` for canonical embedding contracts and deterministic tests. Provider-specific backends belong in the module that naturally owns the backend integration.
