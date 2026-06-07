# rskit-schema

JSON Schema generation and validation helpers for rskit.

`rskit-schema` wraps `schemars` and `jsonschema` behind a small foundational
API for generating schemas from Rust types, compiling schemas once, validating
JSON values, and bounding structured-output validation.

## Features

- Generate JSON Schema documents from `JsonSchema` types.
- Compile schemas into reusable validators.
- Validate arbitrary `serde_json::Value` inputs.
- Configure validation limits for structured outputs.
- Re-export `schemars::JsonSchema` for downstream derive usage.
