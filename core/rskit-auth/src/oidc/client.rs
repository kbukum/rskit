use std::{
    sync::Arc,
    time::{Duration, Instant},
};

use base64::Engine;
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode, decode_header, jwk::JwkSet};
use reqwest::Url;
use serde_json::Value;
use tokio::sync::RwLock;

use super::types::RawOidcClaims;
use super::{
    OidcAuthorizationRequest, OidcClaims, OidcClientType, OidcConfig, OidcError, OidcHttpClient,
    OidcProviderMetadata, OidcTokenExchangeRequest, OidcUserInfo, PkcePair, ReqwestOidcHttpClient,
};

const JWKS_REFRESH_COOLDOWN: Duration = Duration::from_secs(30);

#[derive(Debug, Default)]
struct OidcCache {
    metadata: Option<OidcProviderMetadata>,
    jwks: Option<Arc<JwkSet>>,
    last_forced_jwks_refresh: Option<Instant>,
}

/// Stateful OIDC client with discovery and JWKS caching.
#[derive(Debug)]
pub struct OidcClient<H = ReqwestOidcHttpClient> {
    config: OidcConfig,
    http_client: H,
    cache: Arc<RwLock<OidcCache>>,
}

impl OidcClient<ReqwestOidcHttpClient> {
    /// Create an OIDC client using reqwest.
    ///
    /// # Errors
    /// Returns an error when the configuration is invalid.
    pub fn new(config: OidcConfig) -> Result<Self, OidcError> {
        Self::with_http_client(config, ReqwestOidcHttpClient::new()?)
    }
}

impl<H> OidcClient<H>
where
    H: OidcHttpClient,
{
    /// Create an OIDC client with a caller-supplied HTTP implementation.
    ///
    /// # Errors
    /// Returns an error when the configuration is invalid.
    pub fn with_http_client(config: OidcConfig, http_client: H) -> Result<Self, OidcError> {
        config.validate()?;
        Ok(Self {
            config,
            http_client,
            cache: Arc::new(RwLock::new(OidcCache::default())),
        })
    }

    /// Fetch and cache the provider discovery document.
    ///
    /// # Errors
    /// Returns an error when discovery fails or the document is invalid.
    pub async fn discover(&self) -> Result<OidcProviderMetadata, OidcError> {
        let cached_metadata = self.cache.read().await.metadata.clone();
        if let Some(metadata) = cached_metadata {
            return Ok(metadata);
        }

        let issuer = self.config.issuer.trim_end_matches('/');
        let url = format!("{issuer}/.well-known/openid-configuration");
        let json = self.http_client.get_json(&url, None).await?;
        let metadata = serde_json::from_value::<OidcProviderMetadata>(json).map_err(|error| {
            OidcError::Discovery(format!("invalid discovery document: {error}"))
        })?;
        if metadata.issuer.trim_end_matches('/') != self.config.issuer.trim_end_matches('/') {
            return Err(OidcError::Discovery(
                "provider issuer did not exactly match configured issuer".into(),
            ));
        }
        if !metadata
            .response_types_supported
            .iter()
            .any(|value| value == "code")
        {
            return Err(OidcError::Discovery(
                "provider must support the authorization code flow".into(),
            ));
        }
        if matches!(self.config.client_type, OidcClientType::Public)
            && !metadata
                .code_challenge_methods_supported
                .iter()
                .any(|method| method == "S256")
        {
            return Err(OidcError::Discovery(
                "public clients require PKCE S256 support".into(),
            ));
        }

        self.cache.write().await.metadata = Some(metadata.clone());
        Ok(metadata)
    }

    async fn jwks(&self, force_refresh: bool) -> Result<Arc<JwkSet>, OidcError> {
        if !force_refresh && let Some(jwks) = self.cache.read().await.jwks.clone() {
            return Ok(jwks);
        }

        if force_refresh {
            let mut cache = self.cache.write().await;
            if let Some(jwks) = cache.jwks.clone()
                && cache
                    .last_forced_jwks_refresh
                    .is_some_and(|last_refresh| last_refresh.elapsed() < JWKS_REFRESH_COOLDOWN)
            {
                return Ok(jwks);
            }
            cache.last_forced_jwks_refresh = Some(Instant::now());
        }

        let metadata = self.discover().await?;
        let json = self.http_client.get_json(&metadata.jwks_uri, None).await?;
        let jwks = serde_json::from_value::<JwkSet>(json)
            .map_err(|error| OidcError::Discovery(format!("invalid JWKS document: {error}")))?;
        let jwks = Arc::new(jwks);
        self.cache.write().await.jwks = Some(Arc::clone(&jwks));
        Ok(jwks)
    }

    /// Build a secure authorization request using exact-match redirect URI, state, nonce, and PKCE.
    ///
    /// # Errors
    /// Returns an error when discovery fails.
    pub async fn build_authorization_request(
        &self,
        scopes: &[&str],
    ) -> Result<OidcAuthorizationRequest, OidcError> {
        let metadata = self.discover().await?;
        let state = random_urlsafe(24);
        let nonce = random_urlsafe(24);
        let pkce = Some(PkcePair::generate());

        let mut url = Url::parse(&metadata.authorization_endpoint).map_err(|error| {
            OidcError::Discovery(format!("invalid authorization endpoint: {error}"))
        })?;
        {
            let mut query = url.query_pairs_mut();
            query.append_pair("response_type", "code");
            query.append_pair("client_id", &self.config.client_id);
            query.append_pair("redirect_uri", &self.config.redirect_uri);
            query.append_pair("scope", &scopes.join(" "));
            query.append_pair("state", &state);
            query.append_pair("nonce", &nonce);
            if let Some(pkce) = &pkce {
                query.append_pair("code_challenge", &pkce.challenge);
                query.append_pair("code_challenge_method", pkce.method);
            }
        }

        Ok(OidcAuthorizationRequest {
            url: url.to_string(),
            state,
            nonce,
            pkce,
        })
    }

    /// Build token-exchange parameters while enforcing state and PKCE rules.
    ///
    /// # Errors
    /// Returns an error when the callback state or PKCE data is invalid.
    pub async fn build_token_exchange_request(
        &self,
        pending: &OidcAuthorizationRequest,
        code: &str,
        returned_state: &str,
        code_verifier: Option<&str>,
    ) -> Result<OidcTokenExchangeRequest, OidcError> {
        if pending.state != returned_state {
            return Err(OidcError::StateMismatch);
        }

        let verifier = code_verifier
            .map(ToOwned::to_owned)
            .or_else(|| pending.pkce.as_ref().map(|pkce| pkce.verifier.clone()));

        if matches!(self.config.client_type, OidcClientType::Public) && verifier.is_none() {
            return Err(OidcError::MissingPkce);
        }

        let metadata = self.discover().await?;
        Ok(OidcTokenExchangeRequest {
            token_endpoint: metadata.token_endpoint,
            code: code.to_string(),
            redirect_uri: self.config.redirect_uri.clone(),
            state: returned_state.to_string(),
            code_verifier: verifier,
        })
    }

    /// Validate an ID token using discovery metadata, cached JWKS, and the configured nonce.
    ///
    /// # Errors
    /// Returns an error when the token is invalid or the provider cannot be reached.
    pub async fn validate_id_token(
        &self,
        id_token: &str,
        expected_nonce: Option<&str>,
    ) -> Result<OidcClaims, OidcError> {
        let metadata = self.discover().await?;
        let header = decode_header(id_token)
            .map_err(|error| OidcError::InvalidToken(format!("invalid token header: {error}")))?;

        if !self.config.allowed_algorithms.contains(&header.alg) {
            return Err(OidcError::UnsupportedAlgorithm(format!("{:?}", header.alg)));
        }
        let alg_name = format!("{:?}", header.alg);
        if !metadata.id_token_signing_alg_values_supported.is_empty()
            && !metadata
                .id_token_signing_alg_values_supported
                .iter()
                .any(|value| value == &alg_name)
        {
            return Err(OidcError::UnsupportedAlgorithm(alg_name));
        }

        let jwk = self.select_jwk(header.kid.as_deref()).await?;
        let decoding_key = DecodingKey::from_jwk(&jwk).map_err(|error| {
            OidcError::InvalidToken(format!("could not build decoding key from JWK: {error}"))
        })?;
        let claims = decode::<Value>(
            id_token,
            &decoding_key,
            &oidc_validation(&self.config, header.alg),
        )
        .map_err(|error| map_oidc_jwt_error(&error))?
        .claims;

        let raw_claims = serde_json::from_value::<RawOidcClaims>(claims)
            .map_err(|error| OidcError::InvalidToken(format!("invalid OIDC claims: {error}")))?;
        let iat = raw_claims
            .iat
            .ok_or_else(|| OidcError::MissingClaim("iat".into()))?;
        if let Some(expected_nonce) = expected_nonce
            && raw_claims.nonce.as_deref() != Some(expected_nonce)
        {
            return Err(OidcError::NonceMismatch);
        }

        Ok(OidcClaims {
            sub: raw_claims.sub,
            iss: raw_claims.iss,
            aud: raw_claims.aud.into_vec(),
            exp: raw_claims.exp,
            iat,
            nbf: raw_claims.nbf,
            nonce: raw_claims.nonce,
            email: raw_claims.email,
            email_verified: raw_claims.email_verified,
            name: raw_claims.name,
        })
    }

    async fn select_jwk(&self, kid: Option<&str>) -> Result<jsonwebtoken::jwk::Jwk, OidcError> {
        let kid = kid.ok_or_else(|| {
            OidcError::InvalidToken("token header is missing required kid".into())
        })?;
        for force_refresh in [false, true] {
            let jwks = self.jwks(force_refresh).await?;
            if let Some(jwk) = jwks
                .keys
                .iter()
                .find(|jwk| jwk.common.key_id.as_deref() == Some(kid))
                .cloned()
            {
                return Ok(jwk);
            }
        }
        Err(OidcError::InvalidToken(
            "no matching JWK found for token header".into(),
        ))
    }

    /// Fetch the provider's userinfo document using the bearer access token.
    ///
    /// # Errors
    /// Returns an error when the provider does not expose userinfo or the request fails.
    pub async fn fetch_userinfo(&self, access_token: &str) -> Result<OidcUserInfo, OidcError> {
        let metadata = self.discover().await?;
        let endpoint = metadata.userinfo_endpoint.ok_or_else(|| {
            OidcError::Discovery("provider does not expose a userinfo endpoint".into())
        })?;
        let json = self
            .http_client
            .get_json(&endpoint, Some(access_token))
            .await?;
        serde_json::from_value(json).map_err(|error| {
            OidcError::ProviderUnreachable(format!("invalid userinfo response: {error}"))
        })
    }
}

fn oidc_validation(config: &OidcConfig, algorithm: Algorithm) -> Validation {
    let mut validation = Validation::new(algorithm);
    validation.algorithms = vec![algorithm];
    validation.set_issuer(&[config.issuer.as_str()]);
    validation.set_audience(&config.audience);
    validation.set_required_spec_claims(&["exp", "iss", "aud", "sub"]);
    // Validate nbf when present: tokens with a future nbf are rejected;
    // tokens that omit nbf still pass (nbf is not in required_spec_claims).
    validation.validate_nbf = true;
    validation.leeway = config.clock_skew.as_secs();
    validation
}

fn map_oidc_jwt_error(error: &jsonwebtoken::errors::Error) -> OidcError {
    match error.kind() {
        jsonwebtoken::errors::ErrorKind::ExpiredSignature => {
            OidcError::InvalidToken("OIDC ID token has expired".into())
        }
        jsonwebtoken::errors::ErrorKind::MissingRequiredClaim(claim) => {
            OidcError::MissingClaim(claim.clone())
        }
        _ => OidcError::InvalidToken(error.to_string()),
    }
}

fn random_urlsafe(len: usize) -> String {
    let mut bytes = vec![0_u8; len];
    rand::fill(bytes.as_mut_slice());
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes)
}

/// Validates an OIDC ID token using the default reqwest-based client.
///
/// # Errors
/// Returns an error if the token is invalid, expired, or the provider is unreachable.
pub async fn validate_id_token(
    config: &OidcConfig,
    id_token: &str,
) -> Result<OidcClaims, OidcError> {
    OidcClient::new(config.clone())?
        .validate_id_token(id_token, None)
        .await
}

#[cfg(test)]
mod tests {
    use std::{
        collections::HashMap,
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
    };

    use async_trait::async_trait;
    use jsonwebtoken::{EncodingKey, Header, encode};

    use super::*;

    const ISSUER: &str = "https://issuer.example";
    const CLIENT_ID: &str = "client-123";
    const REDIRECT_URI: &str = "https://app.example/callback";
    const RSA_PRIVATE_KEY: &str = include_str!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/testdata/rsa_private_key.pem"
    ));
    const JWKS_JSON: &str = r#"{
      "keys": [{
        "kty": "RSA",
        "kid": "rsa-1",
        "use": "sig",
        "alg": "RS256",
        "n": "oT6vqY7IFYxu8LYnCgPsGZICOgRF57XQI9wAoILWbGwIBTzZMi_KuWWtYCo-Ph3LhN2vsIUQkKwT1-kob3IfBoCh9kN0iO6f_XLXgAOCCtVyrt5bjLyFJvtYcFecfh80LvdEeL8VE7Fxnd6CoRv1zNszWFxsfSCeWPyWQefJjLcWOqRg5zyFuP5yHwGwzEI3wLKhPFC2ufTupu4GRcFpjKpM3ZxoWw2BEaPcSmKMFWBGd7lsKaBe6TfLvL3pDwzBHSngURGInXrTQGmw-KRpDrcv5vlUD7QwVWP3JWGW66RtWH5qS5MP2eeOOSd48_Yccbx1kbGZ-NQtk6fSOTwd_Q",
        "e": "AQAB"
      }]
    }"#;

    #[derive(Debug, Clone)]
    struct MockHttpClient {
        responses: Arc<HashMap<String, Value>>,
        request_counts: Arc<HashMap<String, AtomicUsize>>,
    }

    #[async_trait]
    impl OidcHttpClient for MockHttpClient {
        async fn get_json(
            &self,
            url: &str,
            _bearer_token: Option<&str>,
        ) -> Result<Value, OidcError> {
            if let Some(counter) = self.request_counts.get(url) {
                counter.fetch_add(1, Ordering::Relaxed);
            }
            self.responses.get(url).cloned().ok_or_else(|| {
                OidcError::ProviderUnreachable(format!("no response configured for {url}"))
            })
        }
    }

    fn mock_client() -> OidcClient<MockHttpClient> {
        let responses = HashMap::from([
            (
                format!("{ISSUER}/.well-known/openid-configuration"),
                serde_json::json!({
                    "issuer": ISSUER,
                    "authorization_endpoint": format!("{ISSUER}/authorize"),
                    "token_endpoint": format!("{ISSUER}/token"),
                    "jwks_uri": format!("{ISSUER}/jwks"),
                    "userinfo_endpoint": format!("{ISSUER}/userinfo"),
                    "response_types_supported": ["code"],
                    "code_challenge_methods_supported": ["S256"],
                    "id_token_signing_alg_values_supported": ["RS256"]
                }),
            ),
            (
                format!("{ISSUER}/jwks"),
                serde_json::from_str(JWKS_JSON).unwrap(),
            ),
            (
                format!("{ISSUER}/userinfo"),
                serde_json::json!({
                    "sub": "user-123",
                    "email": "user@example.com",
                    "email_verified": true,
                    "name": "Example User"
                }),
            ),
        ]);

        OidcClient::with_http_client(
            OidcConfig::new(ISSUER, CLIENT_ID, REDIRECT_URI, OidcClientType::Public),
            MockHttpClient {
                responses: Arc::new(responses),
                request_counts: Arc::new(HashMap::from([
                    (
                        format!("{ISSUER}/.well-known/openid-configuration"),
                        AtomicUsize::new(0),
                    ),
                    (format!("{ISSUER}/jwks"), AtomicUsize::new(0)),
                    (format!("{ISSUER}/userinfo"), AtomicUsize::new(0)),
                ])),
            },
        )
        .unwrap()
    }

    fn issue_token(nonce: &str) -> String {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let mut header = Header::new(Algorithm::RS256);
        header.kid = Some("rsa-1".into());
        encode(
            &header,
            &serde_json::json!({
                "sub": "user-123",
                "iss": ISSUER,
                "aud": [CLIENT_ID],
                "exp": now + 3600,
                "nbf": now.saturating_sub(1),
                "iat": now,
                "nonce": nonce,
                "email": "user@example.com",
                "email_verified": true,
                "name": "Example User"
            }),
            &EncodingKey::from_rsa_pem(RSA_PRIVATE_KEY.as_bytes()).unwrap(),
        )
        .unwrap()
    }

    fn issue_token_without_nbf(nonce: &str) -> String {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let mut header = Header::new(Algorithm::RS256);
        header.kid = Some("rsa-1".into());
        encode(
            &header,
            &serde_json::json!({
                "sub": "user-123",
                "iss": ISSUER,
                "aud": [CLIENT_ID],
                "exp": now + 3600,
                "iat": now,
                "nonce": nonce,
                "email": "user@example.com",
                "email_verified": true,
                "name": "Example User"
            }),
            &EncodingKey::from_rsa_pem(RSA_PRIVATE_KEY.as_bytes()).unwrap(),
        )
        .unwrap()
    }

    #[tokio::test]
    async fn authorization_request_includes_pkce_state_and_nonce() {
        let client = mock_client();
        let request = client
            .build_authorization_request(&["openid", "profile", "email"])
            .await
            .unwrap();

        assert!(request.url.contains("response_type=code"));
        assert!(request.url.contains("code_challenge="));
        assert!(!request.state.is_empty());
        assert!(!request.nonce.is_empty());
    }

    #[tokio::test]
    async fn state_mismatch_is_rejected() {
        let client = mock_client();
        let request = client
            .build_authorization_request(&["openid"])
            .await
            .unwrap();
        let result = client
            .build_token_exchange_request(&request, "code-123", "wrong-state", None)
            .await;
        assert_eq!(result.unwrap_err(), OidcError::StateMismatch);
    }

    #[tokio::test]
    async fn pkce_missing_is_rejected_for_public_client() {
        let client = mock_client();
        let request = OidcAuthorizationRequest {
            url: "https://issuer.example/authorize".into(),
            state: "state-123".into(),
            nonce: "nonce-123".into(),
            pkce: None,
        };
        let result = client
            .build_token_exchange_request(&request, "code-123", "state-123", None)
            .await;
        assert_eq!(result.unwrap_err(), OidcError::MissingPkce);
    }

    #[tokio::test]
    async fn nonce_mismatch_is_rejected() {
        let client = mock_client();
        let token = issue_token("expected-nonce");
        let result = client.validate_id_token(&token, Some("wrong-nonce")).await;
        assert_eq!(result.unwrap_err(), OidcError::NonceMismatch);
    }

    #[tokio::test]
    async fn valid_id_token_and_userinfo_roundtrip() {
        let client = mock_client();
        let token = issue_token("nonce-123");
        let claims = client
            .validate_id_token(&token, Some("nonce-123"))
            .await
            .unwrap();
        let userinfo = client.fetch_userinfo("opaque-access-token").await.unwrap();

        assert_eq!(claims.sub, "user-123");
        assert_eq!(claims.email.as_deref(), Some("user@example.com"));
        assert_eq!(userinfo.name.as_deref(), Some("Example User"));
    }

    #[tokio::test]
    async fn valid_id_token_without_nbf_is_accepted() {
        let client = mock_client();
        let token = issue_token_without_nbf("nonce-123");

        let claims = client
            .validate_id_token(&token, Some("nonce-123"))
            .await
            .unwrap();

        assert_eq!(claims.sub, "user-123");
        assert!(claims.nbf.is_none());
    }

    #[tokio::test]
    async fn id_token_without_kid_is_rejected() {
        let client = mock_client();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let token = encode(
            &Header::new(Algorithm::RS256),
            &serde_json::json!({
                "sub": "user-123",
                "iss": ISSUER,
                "aud": [CLIENT_ID],
                "exp": now + 3600,
                "nbf": now.saturating_sub(1),
                "iat": now,
                "nonce": "nonce-123"
            }),
            &EncodingKey::from_rsa_pem(RSA_PRIVATE_KEY.as_bytes()).unwrap(),
        )
        .unwrap();

        let result = client.validate_id_token(&token, Some("nonce-123")).await;
        assert_eq!(
            result.unwrap_err(),
            OidcError::InvalidToken("token header is missing required kid".into())
        );
    }

    #[tokio::test]
    async fn jwks_forced_refresh_is_throttled_after_unknown_kid() {
        let client = mock_client();
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();
        let mut header = Header::new(Algorithm::RS256);
        header.kid = Some("missing-kid".into());
        let token = encode(
            &header,
            &serde_json::json!({
                "sub": "user-123",
                "iss": ISSUER,
                "aud": [CLIENT_ID],
                "exp": now + 3600,
                "nbf": now.saturating_sub(1),
                "iat": now,
                "nonce": "nonce-123"
            }),
            &EncodingKey::from_rsa_pem(RSA_PRIVATE_KEY.as_bytes()).unwrap(),
        )
        .unwrap();

        let jwks_url = format!("{ISSUER}/jwks");
        let counter = client
            .http_client
            .request_counts
            .get(&jwks_url)
            .expect("jwks counter must exist");

        let first = client.validate_id_token(&token, Some("nonce-123")).await;
        assert_eq!(
            first.unwrap_err(),
            OidcError::InvalidToken("no matching JWK found for token header".into())
        );
        assert_eq!(counter.load(Ordering::Relaxed), 2);

        let second = client.validate_id_token(&token, Some("nonce-123")).await;
        assert_eq!(
            second.unwrap_err(),
            OidcError::InvalidToken("no matching JWK found for token header".into())
        );
        assert_eq!(counter.load(Ordering::Relaxed), 2);
    }
}
