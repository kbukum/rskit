# rskit-auth — Authentication Helpers

JWT signing/verification, OIDC validation, password hashing, API-key helpers, and request-context auth extractors.

[![CI](https://github.com/kbukum/rskit/actions/workflows/ci.yml/badge.svg)](https://github.com/kbukum/rskit/actions/workflows/ci.yml)
[![crates.io](https://img.shields.io/crates/v/rskit-auth.svg)](https://crates.io/crates/rskit-auth)
[![docs.rs](https://docs.rs/rskit-auth/badge.svg)](https://docs.rs/rskit-auth)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![MSRV: 1.85](https://img.shields.io/badge/MSRV-1.85-orange.svg)](https://blog.rust-lang.org/2025/02/20/Rust-1.85.0.html)

## Features

- `JwtService` — sign and verify tokens with explicit `HS256` (internal-only), `RS256`, `ES256`, or `EdDSA`
- `OidcClient` — discovery, PKCE, JWKS-backed ID-token validation, and userinfo lookups
- `PasswordHasher` — Argon2id hashing and verification
- `ResetTokenGenerator` — short-lived random opaque tokens
- `apikey` — prefix lookup + peppered HMAC-SHA-256 digest storage with constant-time compare
- `TokenValidator` / `TokenGenerator` traits for pluggable backends
- `AuthClaims<C>` wrapper for typed claims in Axum request extensions

## Usage

```toml
[dependencies]
rskit-auth = "0.1"
```

```rust
use rskit_auth::{JwtConfig, JwtService, PasswordHasher, TokenGenerator};

# #[derive(serde::Serialize)]
# struct Claims {
#   sub: String,
#   iss: String,
#   aud: Vec<String>,
#   exp: u64,
#   nbf: u64,
#   iat: u64,
# }
let hasher = PasswordHasher::default();
let hash = hasher.hash("s3cret!").unwrap();
assert!(hasher.verify("s3cret!", &hash).unwrap());

let jwt = JwtService::<Claims>::new(JwtConfig::hs256_internal(
    "internal-secret",
    "https://issuer.example",
    vec!["service-a".into()],
))
.unwrap();
```

## JWT / OIDC policy

- Public-key algorithms are preferred: `RS256`, `ES256`, `EdDSA`
- `HS256` remains available only through the explicit `JwtConfig::hs256_internal(...)` constructor
- Verifiers require `sub`, `iss`, `aud`, `exp`, `nbf`, and `iat`
- OIDC enforces authorization-code flow, exact redirect URIs, state, nonce, and PKCE for public clients

## See Also

[Main repository README](https://github.com/kbukum/rskit)
