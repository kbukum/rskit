# rskit-auth — Authentication Helpers

JWT signing/verification, password hashing, and request-context auth extractors.

[![CI](https://github.com/kbukum/rskit/actions/workflows/ci.yml/badge.svg)](https://github.com/kbukum/rskit/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/rskit-auth.svg)](https://crates.io/crates/rskit-auth)
[![docs.rs](https://docs.rs/rskit-auth/badge.svg)](https://docs.rs/rskit-auth)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![MSRV: 1.85](https://img.shields.io/badge/MSRV-1.85-orange.svg)](https://blog.rust-lang.org/2025/02/20/Rust-1.85.0.html)

## Features

- `JwtService` — sign and verify tokens with HMAC (HS256/384/512) or RSA (RS256/384/512)
- `PasswordHasher` — Argon2id hashing and verification
- `ResetTokenGenerator` — short-lived random opaque tokens
- `TokenValidator` / `TokenGenerator` traits for pluggable backends
- `AuthClaims<C>` wrapper for typed claims in Axum request extensions

## Usage

```toml
[dependencies]
rskit-auth = "0.1"
```

```rust
use rskit_auth::{PasswordHasher, ResetTokenGenerator};
use std::time::Duration;

let hasher = PasswordHasher::default();
let hash = hasher.hash("s3cret!").unwrap();
assert!(hasher.verify("s3cret!", &hash).unwrap());

let gen = ResetTokenGenerator::new(Duration::from_secs(300));
let (token, _expiry) = gen.generate();
println!("Reset token: {token}");
```

## See Also

[Main repository README](https://github.com/kbukum/rskit)
