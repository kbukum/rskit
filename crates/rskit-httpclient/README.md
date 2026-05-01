# rskit-httpclient

Async HTTP client for rskit with auth, headers, injected resilience policies, and error handling.

## Features

- Async HTTP client built on `reqwest`
- Support for Bearer, Basic, and API key authentication
- Configurable timeouts, headers, and redirect behavior
- Optional `rskit-resilience::Policy` integration for retry, timeout, circuit breaker, and rate limiting
- URL building with base URL support
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

## Authentication

```rust
use rskit_httpclient::Auth;

// Bearer token
let auth = Auth::bearer("token123");

// HTTP Basic
let auth = Auth::basic("user", "pass");

// API Key
let auth = Auth::api_key("X-API-Key", "key123");

// Per-request override
let resp = client.send(
    Request::get("/api/data")
        .bearer_token("request-specific-token")
).await?;
```

## Error Handling

All methods return `AppResult<T>` (alias for `Result<T, AppError>`). Errors are classified with appropriate `ErrorCode` values:

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
