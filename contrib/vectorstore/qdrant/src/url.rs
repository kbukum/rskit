//! Qdrant endpoint validation.

use rskit_errors::{AppError, AppResult, ErrorCode};
use rskit_httpclient::DestinationPolicy;

pub(crate) fn validate_qdrant_url(url: &str) -> AppResult<()> {
    if url.trim().is_empty() {
        return Err(AppError::new(
            ErrorCode::InvalidInput,
            "Qdrant URL is required",
        ));
    }
    if has_url_credentials(url) || url.contains('?') {
        return Err(AppError::new(
            ErrorCode::InvalidInput,
            "Qdrant URL must not contain credentials or query parameters",
        ));
    }
    let parsed = reqwest::Url::parse(url)
        .map_err(|error| AppError::invalid_input("url", format!("invalid Qdrant URL: {error}")))?;
    DestinationPolicy::default().validate(&parsed)
}

fn has_url_credentials(value: &str) -> bool {
    value
        .split_once("://")
        .and_then(|(_, rest)| rest.split('/').next())
        .is_some_and(|authority| authority.contains('@'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_sensitive_url_forms() {
        assert!(validate_qdrant_url("https://user:pass@qdrant.example.test").is_err());
        assert!(validate_qdrant_url("https://qdrant.example.test?api_key=secret").is_err());
        assert!(validate_qdrant_url("http://169.254.169.254/latest/meta-data").is_err());
        assert!(validate_qdrant_url("http://[fe80::1]:6334").is_err());
        assert!(validate_qdrant_url("ftp://qdrant.example.test").is_err());
    }
}
