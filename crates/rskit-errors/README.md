# rskit-errors — Structured Error Types

`AppError` + `ErrorCode` enum + `AppResult<T>` with HTTP/gRPC status mapping.

[![CI](https://github.com/kbukum/rskit/actions/workflows/ci.yml/badge.svg)](https://github.com/kbukum/rskit/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/rskit-errors.svg)](https://crates.io/crates/rskit-errors)
[![docs.rs](https://docs.rs/rskit-errors/badge.svg)](https://docs.rs/rskit-errors)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![MSRV: 1.85](https://img.shields.io/badge/MSRV-1.85-orange.svg)](https://blog.rust-lang.org/2025/02/20/Rust-1.85.0.html)

## Features

- 17 `ErrorCode` variants covering common application error scenarios
- Fluent builder with `with_detail` / `with_cause`
- Automatic HTTP status code mapping via `http_status()`
- `tonic::Status` conversion for gRPC services
- Retryability query via `is_retryable()`

## Usage

```toml
[dependencies]
rskit-errors = "0.1"
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
