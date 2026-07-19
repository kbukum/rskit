# rskit-httpclient

Async HTTP client for rskit with redacting auth, headers, injected resilience policies,
destination hardening, and error handling.

## Features

- Async HTTP client built on `reqwest`
- Support for Bearer, Basic, and API key authentication with redacted secret storage
- Configurable timeouts, headers, and redirect behavior
- Explicit TLS trust, identity, and minimum-version configuration via `rskit-security`
- Optional `rskit-resilience::Policy` integration for retry, timeout, circuit breaker,
  and rate limiting
- URL building with base URL support and outbound destination validation
- Bounded response body reads to avoid unbounded memory growth
- JSON request/response serialization via `serde`
- Integrated error handling with `rskit-errors`
- Request builder pattern for fluent API

## Usage

```rust
use rskit_httpclient::{HttpClient, HttpClientConfig, Request};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Create client with configuration
    let config = HttpClientConfig::new()
        .with_base_url("https://api.example.com/v1")
        .with_user_agent("my-app/1.0")
        .with_header("X-Custom", "value");

    let client = HttpClient::new(config)?;

    // Simple GET
    let resp = client.get("/users").await?;
    let text = resp.text()?;

    // GET with JSON response
    let data: serde_json::Value = client.get_json("/users").await?;

    // POST with JSON body
    let body = serde_json::json!({"name": "Alice"});
    let resp = client.post("/users", &body).await?;

    // Request with custom auth
    let resp = client.send(
        Request::post("/protected")
            .bearer_token("secret-token")
            .json_body(&body)?
    ).await?;

    // Check status and parse
    let result = resp.error_for_status()?.json::<serde_json::Value>()?;

    Ok(())
}
```

## Authentication and redaction

```rust
use rskit_httpclient::{Auth, HttpClientConfig};
use rskit_security::SecretString;

// Bearer token
let auth = Auth::bearer("token123");

// HTTP Basic
let auth = Auth::basic("user", "pass");

// API Key
let auth = Auth::api_key("X-API-Key", "key123");

// Use *_secret constructors when a secret is already held as a SecretString.
let token = SecretString::new("token123");
let auth = Auth::bearer_secret(token);

// Auth and HttpClientConfig debug output redact credential values.
let config = HttpClientConfig::new().with_auth(auth);
assert!(!format!("{config:?}").contains("token123"));

// Per-request override
let resp = client.send(
    Request::get("/api/data")
        .bearer_token("request-specific-token")
).await?;
```

`Auth::header()` intentionally exposes credential values only at the final request-application boundary.
Do not use `with_header` for bearer/API-key credentials; prefer `with_auth`
or per-request auth helpers so debug output keeps credential fields redacted.

## Transport hardening

`HttpClientConfig` applies safe transport defaults:

- redirects are capped by `max_redirects` (default: 5)
- response bodies are capped by `max_response_body_bytes` (default: 10 MiB)
- `DestinationPolicy` validates initial request URLs and redirect targets
- link-local address literals and common cloud metadata endpoints are blocked by default

Use an allow-list for clients that should only call known hosts:

```rust
use rskit_httpclient::{DestinationPolicy, HttpClientConfig};

let config = HttpClientConfig::new()
    .with_base_url("https://api.example.com")
    .with_destination_policy(
        DestinationPolicy::new().with_allowed_hosts(["api.example.com"])
    )
    .with_max_response_body_bytes(2 * 1024 * 1024);
```

Hostname validation happens before DNS resolution,
so host allow-lists are the preferred protection for high-trust clients that must not follow arbitrary destination names.

## Error Handling

All methods return `AppResult<T>` (alias for `Result<T, AppError>`).
Errors are classified with appropriate `ErrorCode` values:

- `Timeout` for request timeouts
- `ConnectionFailed` for connection errors
- `Unauthorized` for 401 responses
- `Forbidden` for 403 responses
- `NotFound` for 404 responses
- And more...

```rust
match client.get("/users").await {
    Ok(resp) => { /* handle success */ },
    Err(e) => {
        println!("Error code: {}", e.code());
        println!("Message: {}", e.message());
    }
}
```
