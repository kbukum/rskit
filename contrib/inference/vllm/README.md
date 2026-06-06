# rskit-inference-vllm

vLLM REST adapter for `rskit-inference`.

This crate implements the OAI-compatible `/v1/completions` path with explicit
`register(&mut Registry, Config)` factory wiring. It performs no auto-registration and
has no global registry.

## Configuration notes

- `Config::api_key` is an optional `SecretString`; debug/serialization output is redacted.
- When configured, the token is installed through `rskit-httpclient::Auth::bearer_secret`, not a raw header, so HTTP client config debug output does not expose credentials.
- The adapter targets the OpenAI-compatible vLLM endpoint and keeps registration explicit through `register`.
