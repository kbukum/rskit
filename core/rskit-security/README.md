# rskit-security — Shared Security Configuration

Shared TLS and security configuration for rskit transports.

## Features

- `TlsConfig` — certificate, key, CA bundle, server name, and verification settings
- `TlsVersion` — minimum TLS version policy

## Usage

```toml
[dependencies]
rskit-security = "0.1"
```

```rust
use rskit_security::{TlsConfig, TlsVersion};

let tls = TlsConfig {
    ca_file: Some("certs/ca.pem".to_string()),
    server_name: Some("api.example.com".to_string()),
    min_version: TlsVersion::Tls12,
    ..Default::default()
};
tls.validate().unwrap();
```
