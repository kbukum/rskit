# rskit-security — Shared Security Configuration

Shared TLS, secret redaction, and HTTP security vocabulary for rskit transports.

## Features

- `TlsConfig` — certificate, key, CA bundle, server name, and verification settings
- `TlsVersion` — minimum TLS version policy
- `SecretString` — redacting string wrapper for credential-bearing configuration
- `BEARER_AUTH_SCHEME` / `BASIC_AUTH_SCHEME` — shared HTTP auth scheme constants for crates that build or parse `Authorization` values
- `SecurityHeadersConfig` — secure-by-default response header policy

## Usage

```toml
[dependencies]
rskit-security = "0.1"
```

```rust
use rskit_security::{BEARER_AUTH_SCHEME, SecretString, TlsConfig, TlsVersion};

let tls = TlsConfig {
    ca_file: Some("certs/ca.pem".to_string()),
    server_name: Some("api.example.com".to_string()),
    min_version: TlsVersion::Tls12,
    ..Default::default()
};
tls.validate().unwrap();

let api_key = SecretString::new("secret-token");
assert_eq!(api_key.to_string(), "***");
assert_eq!(BEARER_AUTH_SCHEME, "Bearer");
```
