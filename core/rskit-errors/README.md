# rskit-errors — Structured Error Types

`AppError` + `ErrorCode` enum + `AppResult<T>` with RFC 9457 problem details and lightweight HTTP status metadata.

[![CI](https://github.com/kbukum/rskit/actions/workflows/ci.yml/badge.svg)](https://github.com/kbukum/rskit/actions/workflows/ci.yml) [![crates.io](https://img.shields.io/crates/v/rskit-errors.svg)](https://crates.io/crates/rskit-errors) [![docs.rs](https://docs.rs/rskit-errors/badge.svg)](https://docs.rs/rskit-errors) [![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE) [![MSRV: 1.91](https://img.shields.io/badge/MSRV-1.91-orange.svg)](https://github.com/kbukum/rskit/blob/main/core/Cargo.toml)

## Features

- 18 `ErrorCode` variants covering common application error scenarios
- Fluent builder with `with_detail` / `with_cause`
- Automatic HTTP status metadata via `http_status()`
- Retryability query via `is_retryable()`

`rskit-errors` intentionally depends only on lightweight shared protocol/data crates such as `http`, `serde`, and `serde_json`. Transport-specific conversions live in transport crates; for example, `tonic::Status` mapping belongs to `rskit-grpc`.

## Usage

```toml
[dependencies]
rskit-errors = "0.1.0-alpha.1"
```

```rust
use rskit_errors::{AppError, ErrorCode, AppResult};

fn find_user(id: &str) -> AppResult<String> {
    Err(AppError::not_found("user", id)
        .with_detail("tenant", "acme"))
}

let err = find_user("99").unwrap_err();
assert_eq!(err.code, ErrorCode::NotFound);
assert_eq!(err.http_status(), 404);
```

## See Also

[Main repository README](https://github.com/kbukum/rskit)
