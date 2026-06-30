# rskit-validation — Fluent Input Validation

Fluent field-level validator that collects errors and converts to `AppError`.

[![CI](https://github.com/kbukum/rskit/actions/workflows/ci.yml/badge.svg)](https://github.com/kbukum/rskit/actions/workflows/ci.yml) [![crates.io](https://img.shields.io/crates/v/rskit-validation.svg)](https://crates.io/crates/rskit-validation) [![docs.rs](https://docs.rs/rskit-validation/badge.svg)](https://docs.rs/rskit-validation) [![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/kbukum/rskit/blob/main/LICENSE) [![MSRV: 1.91](https://img.shields.io/badge/MSRV-1.91-orange.svg)](https://github.com/kbukum/rskit/blob/main/core/Cargo.toml)

## Features

- Fluent builder API — chain validators on a single `Validator` instance
- String checks: `required`, `min_length`, `max_length`, `email`, `url`, `pattern` (regex)
- UUID checks: `required_uuid`, `optional_uuid`
- Numeric range via `in_range<T>`
- Temporal checks: `before`, `after` (ISO-8601 strings)
- Enum membership via `one_of<T>`
- Custom boolean predicates via `custom`
- Accumulates all `FieldError`s, then converts to `AppError::invalid_input` on `validate()`

## Usage

```toml
[dependencies]
rskit-validation = "0.2.0-alpha.1"
```

```rust
use rskit_validation::Validator;
use rskit_errors::AppResult;

fn validate_signup(name: &str, email: &str, age: u32) -> AppResult<()> {
    Validator::new()
        .required("name", name)
        .min_length("name", name, 3)
        .email("email", email)
        .in_range("age", age, 18u32, 120u32)
        .validate()
}
```

## See Also

[Main repository README](https://github.com/kbukum/rskit)
