# rskit-inference-vllm

vLLM REST adapter for `rskit-inference`.

This crate implements the OAI-compatible `/v1/completions` path with explicit
`register(&mut Registry)` factory wiring. It performs no auto-registration and
has no global registry.
