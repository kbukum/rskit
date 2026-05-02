# rskit-security — Security Headers

Secure-by-default HTTP response headers and transport policy helpers for rskit services.

## Features

- `SecurityHeadersLayer` — tower layer that applies CSP, HSTS, `X-Content-Type-Options`,
  `X-Frame-Options`, `Referrer-Policy`, and `Permissions-Policy`
- `SecurityHeadersConfig` — builder with secure defaults and TLS-aware HSTS policy
- `TransportSecurity` — explicit secure-production vs insecure-local transport mode

## Usage

```toml
[dependencies]
rskit-security = "0.1"
```

```rust
use rskit_security::{SecurityHeadersConfig, SecurityHeadersLayer};

let layer = SecurityHeadersLayer::new(
    SecurityHeadersConfig::default()
        .with_transport_security(rskit_security::TransportSecurity::HttpsOnly),
);
```
