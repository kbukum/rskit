use std::marker::PhantomData;

use async_trait::async_trait;
use rskit_errors::AppResult;
use serde::{Serialize, de::DeserializeOwned};

use super::{JwtCodec, JwtConfig};
use crate::traits::{TokenGenerator, TokenValidator};

/// JWT sign/verify service generic over the claims type `C`.
pub struct JwtService<C> {
    codec: JwtCodec,
    _claims: PhantomData<C>,
}

impl<C> JwtService<C> {
    /// Create a new [`JwtService`] from the given configuration.
    ///
    /// # Errors
    /// Returns an error when key material or validation policy is invalid.
    pub fn new(config: JwtConfig) -> AppResult<Self> {
        Ok(Self {
            codec: JwtCodec::new(config)?,
            _claims: PhantomData,
        })
    }

    /// Return the reusable JWT codec backing this service.
    #[must_use]
    pub const fn codec(&self) -> &JwtCodec {
        &self.codec
    }
}

#[async_trait]
impl<C: Serialize + DeserializeOwned + Send + Sync> TokenGenerator<C> for JwtService<C> {
    async fn generate(&self, claims: &C) -> AppResult<String> {
        self.codec.encode(claims)
    }
}

#[async_trait]
impl<C: Serialize + DeserializeOwned + Send + Sync> TokenValidator<C> for JwtService<C> {
    async fn validate(&self, token: &str) -> AppResult<C> {
        self.codec.decode(token)
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
