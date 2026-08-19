# rskit-auth — Authentication Helpers

JWT signing/verification, OIDC validation, password hashing, API-key helpers, and request-context auth extractors.

[![CI](https://github.com/kbukum/rskit/actions/workflows/ci.yml/badge.svg)](https://github.com/kbukum/rskit/actions/workflows/ci.yml) [![crates.io](https://img.shields.io/crates/v/rskit-auth.svg)](https://crates.io/crates/rskit-auth) [![docs.rs](https://docs.rs/rskit-auth/badge.svg)](https://docs.rs/rskit-auth) [![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](https://github.com/kbukum/rskit/blob/main/LICENSE) [![MSRV: 1.97](https://img.shields.io/badge/MSRV-1.97-orange.svg)](https://github.com/kbukum/rskit/blob/main/core/Cargo.toml)

## Features

- `JwtCodec` / `JwtHeader` —
  rskit-owned JWT encode/decode/header primitives without exposing the underlying JWT library
- `JwtService` — sign and verify tokens with explicit `HS256` (internal-only), `RS256`, `ES256`,
  or `EdDSA`
- `OidcClient` — discovery, PKCE, JWKS-backed ID-token validation, and userinfo lookups
- `PasswordHasher` — Argon2id hashing and verification
- `ResetTokenGenerator` — short-lived random opaque tokens
- `apikey` — prefix lookup + peppered HMAC-SHA-256 digest storage with constant-time compare
- `TokenValidator` / `TokenGenerator` traits for pluggable backends
- `BearerAuthLayer` and `apikey::ApiKeyLayer` — Tower middleware with fail-closed defaults
- `AuthOutcome<C>` / `AuthClaims<C>` — typed request extensions for authenticated
  and explicitly missing credentials

## Usage

```toml
[dependencies]
rskit-auth = "0.2.0-alpha.4"
```

```rust
use rskit_auth::{JwtCodec, JwtConfig, JwtService, PasswordHasher, TokenGenerator};

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
    "internal-secret-key-material-0001",
    "https://issuer.example",
    vec!["service-a".into()],
))
.unwrap();
let codec = JwtCodec::new(JwtConfig::hs256_internal(
    "internal-secret-key-material-0001",
    "https://issuer.example",
    vec!["service-a".into()],
))
.unwrap();
# let _ = (jwt, codec);
```

## JWT / OIDC policy

- Public-key algorithms are preferred: `RS256`, `ES256`, `EdDSA`
- `HS256` remains available only through the explicit `JwtConfig::hs256_internal(...)` constructor
- Verifiers require `sub`, `iss`, `aud`, `exp`, `nbf`, and `iat`; issuer and audience configuration must not be blank
- OIDC enforces authorization-code flow, exact redirect URIs, state, nonce, and PKCE for public clients
- Request middleware extracts credentials from headers only, rejects missing credentials by default, and requires explicit `accept_missing()` for optional authentication
- `BearerAuthLayer` returns a neutral `WWW-Authenticate: Bearer` challenge on rejected requests; downstream applications can add app-specific realm/scope policy at their HTTP boundary if needed
- Credential-bearing Debug output masks bearer/API keys, key digests, PKCE verifiers, authorization codes, and callback secrets

## See Also

[Main repository README](https://github.com/kbukum/rskit)
