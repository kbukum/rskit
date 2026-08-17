# rskit-inference — model-serving runtime contracts

`rskit-inference` is the model-serving runtime contract layer for Triton, vLLM raw, TGI, KServe v2, BentoML, ONNX Runtime Server, TFServing, and custom REST/gRPC serving APIs. Concrete serving adapters live in `contrib/inference/*`. It is not a chat completion abstraction; chat belongs to `rskit-llm` and `contrib/llm/*`.

## Install

```toml
[dependencies]
rskit-inference = "0.2.0-alpha.4"
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

## Adapter crates

| Adapter | Protocol | Streaming | Status |
| --- | --- | --- | --- |
| `rskit-inference` `echo` | local test adapter | No | Implemented in core |
| `rskit-inference-triton` | KServe v2 HTTP | No | Implemented in contrib |
| `rskit-inference-vllm` | OpenAI-compatible `/v1/completions` | No | Implemented in contrib |
| `rskit-inference-tgi` | OpenAI-compatible `/v1/chat/completions` | No | Implemented in contrib |

## Quick start

```rust
use std::collections::HashMap;

use rskit_inference::{Inference, PredictRequest, Value};
use rskit_inference_tgi::{Config, TgiAdapter};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let adapter = TgiAdapter::new(Config {
        base_url: "http://localhost:8080".into(),
        model: "mistral-7b-instruct".into(),
        api_key: None,
        max_tokens: 256,
    })?;

    let response = adapter
        .predict(PredictRequest {
            model_name: String::new(),
            inputs: HashMap::from([(
                "prompt".into(),
                Value::Text {
                    text: "Write a short rollout note.".into(),
                },
            )]),
            ..PredictRequest::default()
        })
        .await?;

    println!("{:?}", response.outputs.get("text"));
    Ok(())
}
```

## When to use

Use `rskit-inference` when you are integrating serving runtimes with typed inputs and outputs. If you want conversational LLM behavior, tool use, or canonical chat events, stay in `rskit-llm`.
