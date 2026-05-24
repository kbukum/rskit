# rskit-inference-tgi

Hugging Face TGI REST adapter for `rskit-inference`.

This crate implements the OAI-compatible `/v1/chat/completions` path with
explicit `register(&mut Registry, Config)` factory wiring. It performs no
auto-registration and has no global registry.
