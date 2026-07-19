use std::sync::Arc;

use rskit_errors::{AppError, AppResult, ErrorCode};
use rskit_security::{TlsConfig, TlsVersion};
use rustls::pki_types::{CertificateDer, PrivateKeyDer, pem::PemObject};
use tokio_rustls::TlsAcceptor;

use crate::http_config::validate_http_tls_config;

pub(super) fn build_tls_acceptor(tls: &TlsConfig) -> AppResult<TlsAcceptor> {
    validate_http_tls_config(tls)?;
    let cert_file = tls.cert_file.as_deref().ok_or_else(|| {
        AppError::invalid_input("tls.cert_file", "cert_file is required for HTTPS serving")
    })?;
    let key_file = tls.key_file.as_deref().ok_or_else(|| {
        AppError::invalid_input("tls.key_file", "key_file is required for HTTPS serving")
    })?;

    let certs = load_certs(cert_file)?;
    let key = load_private_key(key_file)?;
    let versions = match tls.min_version {
        TlsVersion::Tls12 => vec![&rustls::version::TLS13, &rustls::version::TLS12],
        TlsVersion::Tls13 => vec![&rustls::version::TLS13],
        _ => vec![&rustls::version::TLS13],
    };
    let mut config = rustls::ServerConfig::builder_with_protocol_versions(&versions)
        .with_no_client_auth()
        .with_single_cert(certs, key)
        .map_err(|error| {
            AppError::new(
                ErrorCode::InvalidInput,
                format!("invalid HTTP TLS certificate/key pair: {error}"),
            )
            .with_cause(error)
        })?;
    config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];
    Ok(TlsAcceptor::from(Arc::new(config)))
}

fn load_certs(path: &str) -> AppResult<Vec<CertificateDer<'static>>> {
    let certs = CertificateDer::pem_file_iter(path)
        .map_err(|error| {
            AppError::new(
                ErrorCode::InvalidInput,
                format!("failed to load HTTP TLS certificate file '{path}': {error}"),
            )
            .with_cause(error)
        })?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| {
            AppError::new(
                ErrorCode::InvalidInput,
                format!("failed to parse HTTP TLS certificate file '{path}': {error}"),
            )
            .with_cause(error)
        })?;
    if certs.is_empty() {
        return Err(AppError::invalid_input(
            "tls.cert_file",
            "certificate file must contain at least one certificate",
        ));
    }
    Ok(certs)
}

fn load_private_key(path: &str) -> AppResult<PrivateKeyDer<'static>> {
    PrivateKeyDer::from_pem_file(path).map_err(|error| {
        AppError::new(
            ErrorCode::InvalidInput,
            format!("failed to load HTTP TLS key file '{path}': {error}"),
        )
        .with_cause(error)
    })
}

#[cfg(test)]
mod tests {
    use rskit_errors::ErrorCode;
    use rskit_security::TlsConfig;

    use super::{build_tls_acceptor, load_certs, load_private_key};

    #[test]
    fn tls_acceptor_rejects_missing_certificate_paths() {
        let tls = TlsConfig::default();

        let error = match build_tls_acceptor(&tls) {
            Ok(_) => panic!("missing TLS files should be rejected"),
            Err(error) => error,
        };

        assert_eq!(error.code(), ErrorCode::InvalidInput);
        assert!(error.message().contains("cert_file"));
    }

    #[test]
    fn tls_acceptor_rejects_missing_key_path_after_cert_path() {
        let tls = TlsConfig {
            cert_file: Some("missing-cert.pem".to_string()),
            ..Default::default()
        };

        let error = match build_tls_acceptor(&tls) {
            Ok(_) => panic!("missing key file should be rejected before reading files"),
            Err(error) => error,
        };

        assert_eq!(error.code(), ErrorCode::InvalidInput);
        assert!(error.message().contains("key_file"));
    }

    #[test]
    fn tls_loader_reports_missing_certificate_and_key_files() {
        let cert_error = load_certs("missing-cert.pem").unwrap_err();
        assert_eq!(cert_error.code(), ErrorCode::InvalidInput);
        assert!(
            cert_error
                .message()
                .contains("failed to load HTTP TLS certificate")
        );

        let key_error = load_private_key("missing-key.pem").unwrap_err();
        assert_eq!(key_error.code(), ErrorCode::InvalidInput);
        assert!(key_error.message().contains("failed to load HTTP TLS key"));
    }
}
