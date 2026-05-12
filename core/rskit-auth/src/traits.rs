use async_trait::async_trait;
use rskit_errors::AppResult;

/// Validates a bearer token and returns the extracted claims.
#[async_trait]
pub trait TokenValidator<C>: Send + Sync {
    /// Validate `token` and extract claims of type `C`.
    async fn validate(&self, token: &str) -> AppResult<C>;
}

/// Generates a signed bearer token from claims.
#[async_trait]
pub trait TokenGenerator<C>: Send + Sync {
    /// Sign `claims` and return the token string.
    async fn generate(&self, claims: &C) -> AppResult<String>;
}
