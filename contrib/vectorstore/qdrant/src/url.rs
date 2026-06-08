//! Qdrant endpoint validation.

use rskit_errors::{AppError, AppResult, ErrorCode};
use rskit_httpclient::DestinationPolicy;
use url::Url;

/// Validate a Qdrant endpoint URL before constructing the client.
pub(crate) fn validate_qdrant_url(url: &str) -> AppResult<()> {
    if url.trim().is_empty() {
        return Err(AppError::new(
            ErrorCode::InvalidInput,
            "Qdrant URL is required",
        ));
    }
    let parsed = Url::parse(url)
        .map_err(|error| AppError::invalid_input("url", format!("invalid Qdrant URL: {error}")))?;
    if !parsed.username().is_empty()
        || parsed.password().is_some()
        || parsed.query().is_some()
        || parsed.fragment().is_some()
    {
        return Err(AppError::new(
            ErrorCode::InvalidInput,
            "Qdrant URL must not contain credentials, query parameters, or fragments",
        ));
    }
    DestinationPolicy::default().validate(&parsed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_sensitive_url_forms() {
        assert!(validate_qdrant_url("https://user:pass@qdrant.example.test").is_err());
        assert!(validate_qdrant_url("https://qdrant.example.test?api_key=secret").is_err());
        assert!(validate_qdrant_url("https://qdrant.example.test#token=secret").is_err());
        assert!(validate_qdrant_url("http://169.254.169.254/latest/meta-data").is_err());
        assert!(validate_qdrant_url("http://[fe80::1]:6334").is_err());
        assert!(validate_qdrant_url("ftp://qdrant.example.test").is_err());
    }
}
