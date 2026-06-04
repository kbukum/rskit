#![warn(missing_docs)]

//! Async HTTP client with auth, resilience, and error handling.
//!
//! # Features
//!
//! - Async HTTP client built on `reqwest`
//! - Support for Bearer, Basic, and API key authentication
//! - Configurable timeouts, headers, redirects, and injected resilience policies
//! - URL building with base URL support and destination validation
//! - Bounded response-body reads
//! - JSON request/response serialization
//! - Integrated error handling with `rskit-errors`
//!
//! # Example
//!
//! ```no_run
//! use rskit_httpclient::{HttpClient, HttpClientConfig, Request};
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let config = HttpClientConfig::new()
//!         .with_base_url("https://api.example.com")
//!         .with_user_agent("my-app/1.0");
//!
//!     let client = HttpClient::new(config)?;
//!
//!     // Simple GET request
//!     let resp = client.get("/users").await?;
//!     let text = resp.text()?;
//!     println!("{}", text);
//!
//!     // GET request with bearer token
//!     let resp = client.send(
//!         Request::get("/protected")
//!             .bearer_token("secret-token")
//!     ).await?;
//!
//!     // POST request with JSON
//!     let body = serde_json::json!({"name": "Alice"});
//!     let resp = client.post("/users", &body).await?;
//!
//!     Ok(())
//! }
//! ```

pub mod auth;
pub mod client;
pub mod config;
pub mod destination;
pub mod request;
pub mod response;

pub use auth::Auth;
pub use client::HttpClient;
pub use config::HttpClientConfig;
pub use destination::DestinationPolicy;
pub use request::{Request, RequestBody};
pub use response::{ErrorResponse, Response};
