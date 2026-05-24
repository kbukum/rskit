# rskit-testutil — Test Utilities

Test utilities, mock providers, and assertion helpers for rskit services.

[![CI](https://github.com/kbukum/rskit/actions/workflows/ci.yml/badge.svg)](https://github.com/kbukum/rskit/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/rskit-testutil.svg)](https://crates.io/crates/rskit-testutil)
[![docs.rs](https://docs.rs/rskit-testutil/badge.svg)](https://docs.rs/rskit-testutil)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![MSRV: 1.85](https://img.shields.io/badge/MSRV-1.85-orange.svg)](https://blog.rust-lang.org/2025/02/20/Rust-1.85.0.html)

## Features

- `MockProvider<I, O>` — generic mock with `will_return` / `will_fail` queues and call recording
- `assert_ok(result)` — unwrap `AppResult` or panic with context
- `assert_err_code(result, code)` — assert a specific `ErrorCode`
- Thread-safe via `parking_lot::Mutex`

## Usage

```toml
[dev-dependencies]
rskit-testutil = "0.1"
```

```rust
use rskit_testutil::{MockProvider, assert_ok, assert_err_code};
use rskit_errors::ErrorCode;

let mock = MockProvider::<String, u64>::new();
mock.will_return(42);

let result = mock.execute("hello".into());
assert_ok(result);
assert_eq!(mock.call_count(), 1);

mock.will_fail(rskit_errors::AppError::new(ErrorCode::NotFound, "gone"));
assert_err_code(mock.execute("bye".into()), ErrorCode::NotFound);
```

## See Also

[Main repository README](https://github.com/kbukum/rskit)
