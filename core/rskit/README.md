# rskit-suite — Production Rust Toolkit Facade

Pure facade package that re-exports `rskit-*` crates from the Rust crate name `rskit`. It contains no implementation logic; use it when an application wants one dependency entry point instead of importing each foundation crate directly.

[![CI](https://github.com/kbukum/rskit/actions/workflows/ci.yml/badge.svg)](https://github.com/kbukum/rskit/actions/workflows/ci.yml) [![crates.io](https://img.shields.io/crates/v/rskit-suite.svg)](https://crates.io/crates/rskit-suite) [![docs.rs](https://docs.rs/rskit-suite/badge.svg)](https://docs.rs/rskit-suite) [![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/kbukum/rskit/blob/main/LICENSE) [![MSRV: 1.91](https://img.shields.io/badge/MSRV-1.91-orange.svg)](https://www.rust-lang.org/)

## Always-on modules

The default dependency re-exports the domain-free foundation modules: `util`, `errors`, `fs`, `config`, `logging`, `resilience`, `provider`, `pipeline`, `bootstrap`, `component`, `worker`, and `validation`.

## Feature Flags

Optional modules are enabled with Cargo features. Adapter features also enable their owning core abstraction when needed.

| Feature group | Features |
|---------------|----------|
| Transport/service | `server`, `grpc`, `http`, `httpclient`, `sse` |
| Security/composition | `auth`, `authz`, `security`, `encryption`, `di`, `observability`, `discovery` |
| Data/integration | `database`, `cache`, `cache-fs`, `cache-redis`, `messaging`, `messaging-kafka`, `messaging-nats`, `messaging-rabbitmq`, `storage`, `storage-s3`, `storage-gcs`, `vectorstore`, `vectorstore-qdrant` |
| Workflow/runtime | `dag`, `chain`, `process`, `stateful`, `version`, `schema`, `hook` |
| AI/media | `genai`, `prompt`, `llm`, `llm-openai`, `llm-anthropic`, `llm-ollama`, `llm-gemini`, `embedding`, `inference`, `inference-triton`, `inference-vllm`, `inference-tgi`, `skill`, `tool`, `agent`, `mcp`, `media`, `media-ffmpeg`, `media-image`, `media-audio`, `media-full` |
| Tooling | `cli`, `git`, `dataset`, `bench` |
| Aggregate | `full` enables every optional feature |

`rskit-testutil` is intentionally not re-exported. Add it directly as a `dev-dependency` when tests need shared fixtures or harness helpers.

## Usage

```toml
[dependencies]
rskit-suite = "0.2.0-alpha.1"
# or opt into higher-level modules/adapters:
# rskit-suite = { version = "0.2.0-alpha.1", features = ["server", "cli", "storage-s3"] }
```

```rust
use rskit::{AppError, ErrorCode};

let err = AppError::new(ErrorCode::NotFound, "user not found");
assert_eq!(err.code(), ErrorCode::NotFound);
```

## See Also

[Main repository README](https://github.com/kbukum/rskit)
