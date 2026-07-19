//! TLS configuration for the HTTP client: trust roots, client identity, and minimum version.

use std::path::Path;

use rskit_errors::{AppError, AppResult, ErrorCode};
use rskit_fs::sync_io::file;
use rskit_security::{TlsConfig, TlsVersion};

pub(crate) fn apply_tls(
    mut builder: reqwest::ClientBuilder,
    tls: &TlsConfig,
) -> AppResult<reqwest::ClientBuilder> {
    tls.validate()?;
    if tls.server_name.is_some() {
        return Err(AppError::invalid_input(
            "tls.server_name",
            "HTTP client TLS server_name overrides are not supported by reqwest; omit the override so certificate verification uses the URL host",
        ));
    }

    builder = match tls.min_version {
        TlsVersion::Tls12 => builder.min_tls_version(reqwest::tls::Version::TLS_1_2),
        TlsVersion::Tls13 => builder.min_tls_version(reqwest::tls::Version::TLS_1_3),
        _ => builder.min_tls_version(reqwest::tls::Version::TLS_1_3),
    };

    builder = apply_skip_verify(builder, tls.skip_verify)?;

    if let Some(ca_file) = &tls.ca_file {
        let pem = file::read(Path::new(ca_file)).map_err(|error| {
            AppError::new(
                ErrorCode::InvalidInput,
                format!("failed to read HTTP CA bundle '{ca_file}': {error}"),
            )
            .with_cause(error)
        })?;
        let cert = reqwest::Certificate::from_pem(&pem).map_err(|error| {
            AppError::new(
                ErrorCode::InvalidInput,
                format!("invalid HTTP CA bundle '{ca_file}': {error}"),
            )
            .with_cause(error)
        })?;
        builder = builder.add_root_certificate(cert);
    }

    if let (Some(cert_file), Some(key_file)) = (&tls.cert_file, &tls.key_file) {
        let mut pem = file::read(Path::new(cert_file)).map_err(|error| {
            AppError::new(
                ErrorCode::InvalidInput,
                format!("failed to read HTTP client certificate '{cert_file}': {error}"),
            )
            .with_cause(error)
        })?;
        let mut key = file::read(Path::new(key_file)).map_err(|error| {
            AppError::new(
                ErrorCode::InvalidInput,
                format!("failed to read HTTP client key '{key_file}': {error}"),
            )
            .with_cause(error)
        })?;
        pem.append(&mut key);
        let identity = reqwest::Identity::from_pem(&pem).map_err(|error| {
            AppError::new(
                ErrorCode::InvalidInput,
                format!("invalid HTTP client identity '{cert_file}'/'{key_file}': {error}"),
            )
            .with_cause(error)
        })?;
        builder = builder.identity(identity);
    }

    Ok(builder)
}

fn apply_skip_verify(
    builder: reqwest::ClientBuilder,
    skip_verify: bool,
) -> AppResult<reqwest::ClientBuilder> {
    if !skip_verify {
        return Ok(builder);
    }

    #[cfg(all(feature = "danger-tls", debug_assertions))]
    {
        tracing::warn!("HTTP client TLS certificate verification disabled by explicit config");
        Ok(builder.danger_accept_invalid_certs(true))
    }

    #[cfg(not(all(feature = "danger-tls", debug_assertions)))]
    {
        Err(AppError::invalid_input(
            "tls.skip_verify",
            "HTTP client TLS certificate verification can only be disabled in debug builds with the danger-tls feature",
        ))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tls_server_name_override_is_rejected() {
        let tls = TlsConfig {
            server_name: Some("api.internal".to_string()),
            ..Default::default()
        };

        assert!(apply_tls(reqwest::Client::builder(), &tls).is_err());
    }

    #[test]
    fn tls_skip_verify_is_release_guarded() {
        let tls = TlsConfig {
            skip_verify: true,
            ..Default::default()
        };

        let result = apply_tls(reqwest::Client::builder(), &tls);
        if cfg!(all(feature = "danger-tls", debug_assertions)) {
            assert!(result.is_ok());
        } else {
            assert!(result.is_err());
        }
    }

    #[test]
    fn tls13_minimum_is_accepted() {
        let tls = TlsConfig {
            min_version: TlsVersion::Tls13,
            ..Default::default()
        };

        assert!(apply_tls(reqwest::Client::builder(), &tls).is_ok());
    }

    #[test]
    fn missing_ca_bundle_is_rejected() {
        let temp_dir = tempfile::tempdir().unwrap();
        let ca = temp_dir.path().join("missing-ca.pem");
        let tls = TlsConfig {
            ca_file: Some(ca.display().to_string()),
            ..Default::default()
        };

        let error = apply_tls(reqwest::Client::builder(), &tls).expect_err("missing CA bundle");

        assert_eq!(error.code(), ErrorCode::InvalidInput);
        assert!(error.message().contains("failed to read HTTP CA bundle"));
    }

    #[test]
    fn missing_client_identity_files_are_rejected() {
        let temp_dir = tempfile::tempdir().unwrap();
        let cert = temp_dir.path().join("client.pem");
        let key = temp_dir.path().join("client.key");
        let tls = TlsConfig {
            cert_file: Some(cert.display().to_string()),
            key_file: Some(key.display().to_string()),
            ..Default::default()
        };

        let error =
            apply_tls(reqwest::Client::builder(), &tls).expect_err("missing identity files");

        assert_eq!(error.code(), ErrorCode::InvalidInput);
        assert!(
            error
                .message()
                .contains("failed to read HTTP client certificate")
        );
    }

    #[test]
    fn invalid_client_identity_is_rejected() {
        let cert = tempfile::NamedTempFile::new().unwrap();
        let key = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(cert.path(), b"not a cert").unwrap();
        std::fs::write(key.path(), b"not a key").unwrap();
        let tls = TlsConfig {
            cert_file: Some(cert.path().display().to_string()),
            key_file: Some(key.path().display().to_string()),
            ..Default::default()
        };

        let error = apply_tls(reqwest::Client::builder(), &tls).expect_err("invalid identity");

        assert_eq!(error.code(), ErrorCode::InvalidInput);
        assert!(error.message().contains("invalid HTTP client identity"));
    }
}
