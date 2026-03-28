use std::marker::PhantomData;

use async_trait::async_trait;
use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation};
use rskit_errors::{AppError, AppResult};
use serde::{de::DeserializeOwned, Serialize};

use super::config::JwtConfig;
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
    pub fn new(config: JwtConfig) -> Self {
        let encoding_key = EncodingKey::from_secret(config.secret.as_bytes());
        let decoding_key = DecodingKey::from_secret(config.secret.as_bytes());
        Self { config, encoding_key, decoding_key, _claims: PhantomData }
    }
}

#[async_trait]
impl<C: Serialize + DeserializeOwned + Send + Sync> TokenGenerator<C> for JwtService<C> {
    async fn generate(&self, claims: &C) -> AppResult<String> {
        let algo = (&self.config.algorithm).into();
        let header = Header::new(algo);
        jsonwebtoken::encode(&header, claims, &self.encoding_key).map_err(|e| {
            AppError::new(rskit_errors::ErrorCode::Internal, format!("JWT encode error: {e}"))
        })
    }
}

#[async_trait]
impl<C: Serialize + DeserializeOwned + Send + Sync> TokenValidator<C> for JwtService<C> {
    async fn validate(&self, token: &str) -> AppResult<C> {
        let algo = (&self.config.algorithm).into();
        let mut validation = Validation::new(algo);

        if let Some(iss) = &self.config.issuer {
            validation.set_issuer(&[iss]);
        }
        if let Some(aud) = &self.config.audience {
            validation.set_audience(aud);
        }

        let data = jsonwebtoken::decode::<C>(token, &self.decoding_key, &validation)
            .map_err(|e| match e.kind() {
                jsonwebtoken::errors::ErrorKind::ExpiredSignature => {
                    AppError::token_expired()
                }
                _ => AppError::invalid_token(),
            })?;

        Ok(data.claims)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
    struct TestClaims {
        sub: String,
        exp: u64,
    }

    fn make_service() -> JwtService<TestClaims> {
        JwtService::new(JwtConfig {
            secret: "test-secret-key".into(),
            ..Default::default()
        })
    }

    #[tokio::test]
    async fn roundtrip_sign_and_verify() {
        let svc = make_service();
        let claims = TestClaims {
            sub: "user-123".into(),
            exp: (std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs())
                + 3600,
        };

        let token = svc.generate(&claims).await.unwrap();
        let decoded = svc.validate(&token).await.unwrap();
        assert_eq!(decoded, claims);
    }

    #[tokio::test]
    async fn expired_token_returns_token_expired_error() {
        let svc = make_service();
        let claims = TestClaims {
            sub: "user-123".into(),
            exp: 1, // already expired
        };
        let token = svc.generate(&claims).await.unwrap();
        let result = svc.validate(&token).await;
        assert!(result.is_err());
    }
}
