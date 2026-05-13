use std::marker::PhantomData;

use async_trait::async_trait;
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation};
use rskit_errors::{AppError, AppResult, ErrorCode};
use serde::{Serialize, de::DeserializeOwned};

use super::config::{JwtConfig, JwtKeyMaterial};
use crate::traits::{TokenGenerator, TokenValidator};

/// JWT sign/verify service generic over the claims type `C`.
pub struct JwtService<C> {
    config: JwtConfig,
    encoding_key: EncodingKey,
    decoding_key: DecodingKey,
    _claims: PhantomData<C>,
}

impl<C> JwtService<C> {
    /// Create a new [`JwtService`] from the given configuration.
    ///
    /// # Errors
    /// Returns an error when key material or validation policy is invalid.
    pub fn new(config: JwtConfig) -> AppResult<Self> {
        validate_config(&config)?;
        let (encoding_key, decoding_key) = build_keys(&config.key_material)?;
        Ok(Self {
            config,
            encoding_key,
            decoding_key,
            _claims: PhantomData,
        })
    }
}

fn validate_config(config: &JwtConfig) -> AppResult<()> {
    if config.issuer.trim().is_empty() {
        return Err(AppError::invalid_input(
            "issuer",
            "issuer must not be empty",
        ));
    }
    if config.audience.is_empty() {
        return Err(AppError::invalid_input(
            "audience",
            "at least one audience value is required",
        ));
    }
    if config.leeway.as_secs() > 60 {
        return Err(AppError::invalid_input(
            "leeway",
            "clock skew tolerance must be 60 seconds or less",
        ));
    }
    Ok(())
}

fn build_keys(key_material: &JwtKeyMaterial) -> AppResult<(EncodingKey, DecodingKey)> {
    match key_material {
        JwtKeyMaterial::Hs256Internal { secret } => {
            if secret.is_empty() {
                return Err(AppError::invalid_input(
                    "secret",
                    "HMAC secret must not be empty",
                ));
            }
            if secret.len() < 32 {
                return Err(AppError::invalid_input(
                    "secret",
                    "HMAC secret must be at least 32 bytes",
                ));
            }
            Ok((
                EncodingKey::from_secret(secret.expose().as_bytes()),
                DecodingKey::from_secret(secret.expose().as_bytes()),
            ))
        }
        JwtKeyMaterial::Rs256 {
            private_key_pem,
            public_key_pem,
        } => Ok((
            EncodingKey::from_rsa_pem(private_key_pem.expose().as_bytes())
                .map_err(|error| jwt_key_error(&error))?,
            DecodingKey::from_rsa_pem(public_key_pem.expose().as_bytes())
                .map_err(|error| jwt_key_error(&error))?,
        )),
        JwtKeyMaterial::Es256 {
            private_key_pem,
            public_key_pem,
        } => Ok((
            EncodingKey::from_ec_pem(private_key_pem.expose().as_bytes())
                .map_err(|error| jwt_key_error(&error))?,
            DecodingKey::from_ec_pem(public_key_pem.expose().as_bytes())
                .map_err(|error| jwt_key_error(&error))?,
        )),
        JwtKeyMaterial::EdDsa {
            private_key_pem,
            public_key_pem,
        } => Ok((
            EncodingKey::from_ed_pem(private_key_pem.expose().as_bytes())
                .map_err(|error| jwt_key_error(&error))?,
            DecodingKey::from_ed_pem(public_key_pem.expose().as_bytes())
                .map_err(|error| jwt_key_error(&error))?,
        )),
    }
}

fn jwt_key_error(error: &jsonwebtoken::errors::Error) -> AppError {
    AppError::new(
        ErrorCode::InvalidInput,
        format!("invalid JWT key material: {error}"),
    )
}

fn validation_for(config: &JwtConfig) -> Validation {
    let mut validation = Validation::new(config.algorithm().as_jsonwebtoken());
    validation.leeway = config.leeway.as_secs();
    validation.validate_nbf = true;
    validation.set_issuer(&[config.issuer.as_str()]);
    validation.set_audience(&config.audience);
    // Include "iat" in required claims to avoid a second decode pass.
    validation.set_required_spec_claims(&["exp", "nbf", "iss", "aud", "sub", "iat"]);
    validation.algorithms = vec![config.algorithm().as_jsonwebtoken()];
    validation
}

fn map_validation_error(error: &jsonwebtoken::errors::Error) -> AppError {
    match error.kind() {
        jsonwebtoken::errors::ErrorKind::ExpiredSignature => AppError::token_expired(),
        jsonwebtoken::errors::ErrorKind::InvalidAlgorithm
        | jsonwebtoken::errors::ErrorKind::InvalidSignature
        | jsonwebtoken::errors::ErrorKind::InvalidToken
        | jsonwebtoken::errors::ErrorKind::InvalidIssuer
        | jsonwebtoken::errors::ErrorKind::InvalidAudience
        | jsonwebtoken::errors::ErrorKind::ImmatureSignature
        | jsonwebtoken::errors::ErrorKind::MissingRequiredClaim(_) => AppError::invalid_token(),
        _ => AppError::new(
            ErrorCode::Unauthorized,
            format!("JWT validation failed: {error}"),
        ),
    }
}

#[async_trait]
impl<C: Serialize + DeserializeOwned + Send + Sync> TokenGenerator<C> for JwtService<C> {
    async fn generate(&self, claims: &C) -> AppResult<String> {
        let mut header = Header::new(self.config.algorithm().as_jsonwebtoken());
        header.typ = Some("JWT".to_string());
        jsonwebtoken::encode(&header, claims, &self.encoding_key).map_err(|error| {
            AppError::new(
                ErrorCode::Internal,
                format!(
                    "JWT encode error for {:?}: {error}",
                    self.config.algorithm()
                ),
            )
        })
    }
}

#[async_trait]
impl<C: Serialize + DeserializeOwned + Send + Sync> TokenValidator<C> for JwtService<C> {
    async fn validate(&self, token: &str) -> AppResult<C> {
        let header =
            jsonwebtoken::decode_header(token).map_err(|error| map_validation_error(&error))?;
        let configured_algorithm = self.config.algorithm().as_jsonwebtoken();
        if header.alg != configured_algorithm {
            return Err(AppError::invalid_token().context("JWT algorithm mismatch"));
        }

        let validation = validation_for(&self.config);
        let data = jsonwebtoken::decode::<C>(token, &self.decoding_key, &validation)
            .map_err(|error| map_validation_error(&error))?;
        Ok(data.claims)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    const ISSUER: &str = "https://issuer.example";
    const AUDIENCE: &str = "rskit-tests";

    const RSA_PRIVATE_KEY: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/testdata/rsa_private_key.pem"
    ));
    const RSA_PUBLIC_KEY: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/testdata/rsa_public_key.pem"
    ));

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
    struct TestClaims {
        sub: String,
        iss: String,
        aud: Vec<String>,
        exp: u64,
        nbf: u64,
        iat: u64,
    }

    fn future_timestamp() -> u64 {
        (std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs())
            + 3600
    }

    fn current_timestamp() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
    }

    fn symmetric_service() -> JwtService<TestClaims> {
        JwtService::new(JwtConfig::hs256_internal(
            "test-secret-key-32-bytes-minimum!",
            ISSUER,
            vec![AUDIENCE.to_string()],
        ))
        .unwrap()
    }

    #[tokio::test]
    async fn roundtrip_sign_and_verify() {
        let svc = symmetric_service();
        let now = current_timestamp();
        let claims = TestClaims {
            sub: "user-123".into(),
            iss: ISSUER.into(),
            aud: vec![AUDIENCE.into()],
            exp: future_timestamp(),
            nbf: now.saturating_sub(1),
            iat: now,
        };

        let token = svc.generate(&claims).await.unwrap();
        let decoded = svc.validate(&token).await.unwrap();
        assert_eq!(decoded, claims);
    }

    #[tokio::test]
    async fn expired_token_returns_token_expired_error() {
        let svc = symmetric_service();
        let now = current_timestamp();
        let claims = TestClaims {
            sub: "user-123".into(),
            iss: ISSUER.into(),
            aud: vec![AUDIENCE.into()],
            exp: 1,
            nbf: now.saturating_sub(1),
            iat: now,
        };
        let token = svc.generate(&claims).await.unwrap();
        let result = svc.validate(&token).await;
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn rs256_roundtrip_is_supported() {
        let svc = JwtService::<TestClaims>::new(JwtConfig::rs256(
            RSA_PRIVATE_KEY,
            RSA_PUBLIC_KEY,
            ISSUER,
            vec![AUDIENCE.to_string()],
        ))
        .unwrap();
        let now = current_timestamp();
        let claims = TestClaims {
            sub: "user-123".into(),
            iss: ISSUER.into(),
            aud: vec![AUDIENCE.into()],
            exp: future_timestamp(),
            nbf: now.saturating_sub(1),
            iat: now,
        };
        let token = svc.generate(&claims).await.unwrap();
        let decoded = svc.validate(&token).await.unwrap();
        assert_eq!(decoded.sub, "user-123");
    }

    #[tokio::test]
    async fn alg_none_is_rejected() {
        let svc = symmetric_service();
        let token = concat!(
            "eyJhbGciOiJub25lIiwidHlwIjoiSldUIn0.",
            "eyJzdWIiOiJ1c2VyLTEyMyIsImlzcyI6Imh0dHBzOi8vaXNzdWVyLmV4YW1wbGUiLCJhdWQiOlsicnNraXQtdGVzdHMiXSwiZXhwIjo0MTAyNDQ0ODAwLCJuYmYiOjE3MDAwMDAwMDAsImlhdCI6MTcwMDAwMDAwMH0.",
            ""
        );

        let result = svc.validate(token).await;
        assert!(result.is_err());
    }
}
