# rskit-llm-common — shared LLM adapter internals

`rskit-llm-common` contains shared parsing, error, token-estimation, OpenAI-compatible dialect, stream accumulation, and chat-runner helpers used by the contrib LLM adapters.

## Install

Most applications should not depend on this crate directly. Use one of the provider adapters instead:

```toml
[dependencies]
rskit-llm-openai = "0.1.0-alpha.2"
rskit-llm-anthropic = "0.1.0-alpha.2"
rskit-llm-gemini = "0.1.0-alpha.2"
rskit-llm-ollama = "0.1.0-alpha.2"
```

## When to use

Use this crate only when building or maintaining an LLM adapter inside rskit. Public application code should use `rskit-llm` contracts and register concrete adapters explicitly.
