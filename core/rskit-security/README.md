# rskit-security — Security Headers, CORS, and Input Hardening

Secure-by-default HTTP response headers, deny-by-default CORS, and input hardening helpers for rskit services.

## Features

- `SecurityHeadersLayer` — tower layer that applies CSP, HSTS, `X-Content-Type-Options`,
  `X-Frame-Options`, `Referrer-Policy`, and `Permissions-Policy`
- `SecurityHeadersConfig` — builder with secure defaults and TLS-aware HSTS policy
- `TransportSecurity` — explicit secure-production vs insecure-local transport mode
- `CorsConfig` — explicit CORS policy that denies by default and rejects wildcard origins
- `validate_safe_path()` — reusable path traversal prevention
- `reject_dangerous_unicode()` — rejects RTL controls and common confusable characters

## Usage

```toml
[dependencies]
rskit-security = "0.1"
```

```rust
use rskit_security::{
    CorsConfig, SecurityHeadersConfig, SecurityHeadersLayer, TransportSecurity, validate_safe_path,
};

let config = SecurityHeadersConfig::default().with_transport_security(TransportSecurity::HttpsOnly);
let layer = SecurityHeadersLayer::new(&config).unwrap();

let cors = CorsConfig::default();
let _cors_layer = cors.layer().unwrap();

validate_safe_path("tenant/report.json").unwrap();
```
