//! OpenID Connect (OIDC) support — discovery, PKCE, token validation, and userinfo.

use std::{sync::Arc, time::Duration};

use async_trait::async_trait;
use base64::Engine;
use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode, decode_header, jwk::JwkSet};
use reqwest::{Client, Url};
use serde::Deserialize;
use serde_json::Value;
use sha2::Digest;
use tokio::sync::RwLock;

/// `OpenID` Connect client type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
#[non_exhaustive]
pub enum OidcClientType {
    /// Public/native/browser clients. PKCE is mandatory.
    #[default]
    Public,
    /// Confidential server-side clients. PKCE is still recommended.
    Confidential,
}

/// OIDC validation and authorization configuration.
#[derive(Debug, Clone)]
pub struct OidcConfig {
    /// Issuer URL (e.g. `https://accounts.google.com`).
    pub issuer: String,
    /// Client ID registered with the provider.
    pub client_id: String,
    /// Exact redirect URI registered with the provider.
    pub redirect_uri: String,
    /// OIDC client type.
    pub client_type: OidcClientType,
    /// Accepted audience claim values.
    pub audience: Vec<String>,
    /// Accepted ID-token signing algorithms.
    pub allowed_algorithms: Vec<Algorithm>,
    /// Clock-skew tolerance for `exp` and `nbf`.
    pub clock_skew: Duration,
}

impl OidcConfig {
    /// Create a new OIDC configuration.
    #[must_use]
    pub fn new(
        issuer: impl Into<String>,
        client_id: impl Into<String>,
        redirect_uri: impl Into<String>,
        client_type: OidcClientType,
    ) -> Self {
        let client_id = client_id.into();
        Self {
            issuer: issuer.into(),
            audience: vec![client_id.clone()],
            client_id,
            redirect_uri: redirect_uri.into(),
            client_type,
            allowed_algorithms: vec![Algorithm::RS256, Algorithm::ES256, Algorithm::EdDSA],
            clock_skew: Duration::from_secs(30),
        }
    }

    /// Override accepted audiences.
    #[must_use]
    pub fn with_audience(mut self, audience: Vec<String>) -> Self {
        self.audience = audience;
        self
    }

    /// Override allowed algorithms.
    #[must_use]
    pub fn with_allowed_algorithms(mut self, algorithms: Vec<Algorithm>) -> Self {
        self.allowed_algorithms = algorithms;
        self
    }

    /// Override clock-skew tolerance.
    #[must_use]
    pub const fn with_clock_skew(mut self, clock_skew: Duration) -> Self {
        self.clock_skew = clock_skew;
        self
    }

    fn validate(&self) -> Result<(), OidcError> {
        if self.clock_skew.as_secs() > 60 {
            return Err(OidcError::Configuration(
                "clock skew tolerance must be 60 seconds or less".into(),
            ));
        }
        let issuer = Url::parse(&self.issuer)
            .map_err(|error| OidcError::Configuration(format!("invalid issuer URL: {error}")))?;
        let redirect_uri = Url::parse(&self.redirect_uri)
            .map_err(|error| OidcError::Configuration(format!("invalid redirect URI: {error}")))?;

        if issuer.scheme() != "https" {
            return Err(OidcError::Configuration(
                "OIDC issuer must use HTTPS".into(),
            ));
        }
        if !is_allowed_redirect_uri(&redirect_uri) {
            return Err(OidcError::Configuration(
                "redirect URI must be HTTPS or a localhost development callback".into(),
            ));
        }
        if self.audience.is_empty() {
            return Err(OidcError::Configuration(
                "OIDC audience must not be empty".into(),
            ));
        }
        Ok(())
    }
}

fn is_allowed_redirect_uri(url: &Url) -> bool {
    if url.scheme() == "https" {
        return true;
    }
    if url.scheme() == "http"
        && let Some(host) = url.host_str()
    {
        return matches!(host, "localhost" | "127.0.0.1" | "::1");
    }
    false
}

/// OIDC discovery document.
#[derive(Debug, Clone, Deserialize)]
pub struct OidcProviderMetadata {
    /// Issuer URL.
    pub issuer: String,
    /// Authorization endpoint.
    pub authorization_endpoint: String,
    /// Token endpoint.
    pub token_endpoint: String,
    /// JWKS endpoint.
    pub jwks_uri: String,
    /// Userinfo endpoint.
    pub userinfo_endpoint: Option<String>,
    /// Supported response types.
    #[serde(default)]
    pub response_types_supported: Vec<String>,
    /// Supported PKCE code challenge methods.
    #[serde(default)]
    pub code_challenge_methods_supported: Vec<String>,
    /// Supported ID-token signing algorithms.
    #[serde(default)]
    pub id_token_signing_alg_values_supported: Vec<String>,
}

/// Built authorization request plus correlated anti-CSRF state.
#[derive(Debug, Clone)]
pub struct OidcAuthorizationRequest {
    /// Fully rendered authorization URL.
    pub url: String,
    /// Anti-CSRF state value.
    pub state: String,
    /// Nonce that must be echoed in the ID token.
    pub nonce: String,
    /// PKCE verifier/challenge pair.
    pub pkce: Option<PkcePair>,
}

/// Token exchange parameters validated against the authorization request state.
#[derive(Debug, Clone)]
pub struct OidcTokenExchangeRequest {
    /// Token endpoint discovered from the provider.
    pub token_endpoint: String,
    /// Authorization code from the callback.
    pub code: String,
    /// Exact redirect URI configured for the client.
    pub redirect_uri: String,
    /// Original anti-CSRF state.
    pub state: String,
    /// Optional PKCE verifier. Required for public clients.
    pub code_verifier: Option<String>,
}

/// PKCE verifier/challenge pair.
#[derive(Debug, Clone)]
pub struct PkcePair {
    /// High-entropy verifier.
    pub verifier: String,
    /// SHA-256 based challenge.
    pub challenge: String,
    /// Challenge method, always `S256`.
    pub method: &'static str,
}

impl PkcePair {
    /// Generate a PKCE verifier/challenge pair.
    #[must_use]
    pub fn generate() -> Self {
        let mut bytes = [0_u8; 32];
        rand::fill(&mut bytes);
        let verifier = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes);
        let challenge = base64::engine::general_purpose::URL_SAFE_NO_PAD
            .encode(sha2::Sha256::digest(verifier.as_bytes()));
        Self {
            verifier,
            challenge,
            method: "S256",
        }
    }
}

/// Claims extracted from a validated OIDC ID token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OidcClaims {
    /// Subject identifier.
    pub sub: String,
    /// Issuer identifier.
    pub iss: String,
    /// Audience values.
    pub aud: Vec<String>,
    /// Expiration timestamp (seconds since epoch).
    pub exp: u64,
    /// Issued-at timestamp.
    pub iat: u64,
    /// Not-before timestamp, if present.
    pub nbf: Option<u64>,
    /// OIDC nonce claim, if present.
    pub nonce: Option<String>,
    /// User email, if present.
    pub email: Option<String>,
    /// Whether the provider verified the email.
    pub email_verified: Option<bool>,
    /// Human-readable display name.
    pub name: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct RawOidcClaims {
    sub: String,
    iss: String,
    aud: Audiences,
    exp: u64,
    iat: Option<u64>,
    nbf: Option<u64>,
    nonce: Option<String>,
    email: Option<String>,
    email_verified: Option<bool>,
    name: Option<String>,
}

/// User profile returned by the OIDC userinfo endpoint.
#[derive(Debug, Clone, Deserialize, PartialEq, Eq)]
pub struct OidcUserInfo {
    /// Subject identifier.
    pub sub: String,
    /// User email, if present.
    pub email: Option<String>,
    /// Whether the provider verified the email.
    pub email_verified: Option<bool>,
    /// Human-readable name.
    pub name: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
enum Audiences {
    One(String),
    Many(Vec<String>),
}

impl Audiences {
    fn into_vec(self) -> Vec<String> {
        match self {
            Self::One(value) => vec![value],
            Self::Many(values) => values,
        }
    }
}

#[async_trait]
/// Minimal async HTTP client contract used by OIDC.
pub trait OidcHttpClient: Send + Sync {
    /// Fetch JSON from a URL, optionally using bearer authentication.
    async fn get_json(&self, url: &str, bearer_token: Option<&str>) -> Result<Value, OidcError>;
}

/// Default reqwest-backed OIDC HTTP client.
#[derive(Debug, Clone)]
pub struct ReqwestOidcHttpClient {
    client: Client,
}

impl Default for ReqwestOidcHttpClient {
    fn default() -> Self {
        Self {
            client: Client::new(),
        }
    }
}

#[async_trait]
impl OidcHttpClient for ReqwestOidcHttpClient {
    async fn get_json(&self, url: &str, bearer_token: Option<&str>) -> Result<Value, OidcError> {
        let mut request = self.client.get(url);
        if let Some(token) = bearer_token {
            request = request.bearer_auth(token);
        }
        let response = request
            .send()
            .await
            .map_err(|error| OidcError::ProviderUnreachable(error.to_string()))?;
        let status = response.status();
        if !status.is_success() {
            return Err(OidcError::ProviderUnreachable(format!(
                "provider returned HTTP {status}"
            )));
        }
        response
            .json::<Value>()
            .await
            .map_err(|error| OidcError::ProviderUnreachable(error.to_string()))
    }
}

#[derive(Debug, Default)]
struct OidcCache {
    metadata: Option<OidcProviderMetadata>,
    jwks: Option<JwkSet>,
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
        Self::with_http_client(config, ReqwestOidcHttpClient::default())
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

    async fn jwks(&self, force_refresh: bool) -> Result<JwkSet, OidcError> {
        if !force_refresh && let Some(jwks) = self.cache.read().await.jwks.clone() {
            return Ok(jwks);
        }

        let metadata = self.discover().await?;
        let json = self.http_client.get_json(&metadata.jwks_uri, None).await?;
        let jwks = serde_json::from_value::<JwkSet>(json)
            .map_err(|error| OidcError::Discovery(format!("invalid JWKS document: {error}")))?;
        self.cache.write().await.jwks = Some(jwks.clone());
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
        for force_refresh in [false, true] {
            let jwks = self.jwks(force_refresh).await?;
            if let Some(kid) = kid {
                if let Some(jwk) = jwks
                    .keys
                    .into_iter()
                    .find(|jwk| jwk.common.key_id.as_deref() == Some(kid))
                {
                    return Ok(jwk);
                }
            } else if let Some(jwk) = jwks.keys.into_iter().next() {
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
    // nbf is optional in OIDC — many providers omit it; only exp/iss/aud/sub/iat are required.
    validation.set_required_spec_claims(&["exp", "iss", "aud", "sub"]);
    validation.validate_nbf = false;
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

/// Errors from OIDC operations.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum OidcError {
    /// Configuration is invalid or unsafe.
    #[error("invalid OIDC configuration: {0}")]
    Configuration(String),
    /// Discovery failed or returned an invalid document.
    #[error("OIDC discovery failed: {0}")]
    Discovery(String),
    /// The token could not be validated.
    #[error("invalid token: {0}")]
    InvalidToken(String),
    /// The OIDC provider could not be reached.
    #[error("provider unreachable: {0}")]
    ProviderUnreachable(String),
    /// Callback state mismatched the original request.
    #[error("OIDC state mismatch")]
    StateMismatch,
    /// Public clients must supply PKCE verifiers.
    #[error("PKCE verifier is required for public clients")]
    MissingPkce,
    /// Nonce mismatched the validated ID token.
    #[error("OIDC nonce mismatch")]
    NonceMismatch,
    /// The provider or token selected an unsupported algorithm.
    #[error("unsupported OIDC signing algorithm: {0}")]
    UnsupportedAlgorithm(String),
    /// A required claim is missing.
    #[error("missing required OIDC claim: {0}")]
    MissingClaim(String),
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use jsonwebtoken::{EncodingKey, Header, encode};

    const ISSUER: &str = "https://issuer.example";
    const CLIENT_ID: &str = "client-123";
    const REDIRECT_URI: &str = "https://app.example/callback";
    const RSA_PRIVATE_KEY: &str = r"-----BEGIN PRIVATE KEY-----
MIIEvQIBADANBgkqhkiG9w0BAQEFAASCBKcwggSjAgEAAoIBAQChPq+pjsgVjG7w
ticKA+wZkgI6BEXntdAj3ACggtZsbAgFPNkyL8q5Za1gKj4+HcuE3a+whRCQrBPX
6Shvch8GgKH2Q3SI7p/9cteAA4IK1XKu3luMvIUm+1hwV5x+HzQu90R4vxUTsXGd
3oKhG/XM2zNYXGx9IJ5Y/JZB58mMtxY6pGDnPIW4/nIfAbDMQjfAsqE8ULa59O6m
7gZFwWmMqkzdnGhbDYERo9xKYowVYEZ3uWwpoF7pN8u8vekPDMEdKeBREYidetNA
abD4pGkOty/m+VQPtDBVY/clYZbrpG1YfmpLkw/Z5445J3jz9hxxvHWRsZn41C2T
p9I5PB39AgMBAAECggEAJZ4jyjl62XghW7wLQI1otBB5v4JGsseabvtBFDFiB/pu
JparD0fSEk2z1JcWXVSDKhS0C8kHc9EJcho9qj5kGZbB8luLYPTW04DK4N0wpgll
D4HltuA2akFCQTdXVZ8/A+QBD/B4YNdJa+zA6ghFYI0VHfy1/L0y5AzNm0ORpGel
jJ/31SQnQgud8GPicWLA1TB53zM6TkidTMQWSDGazkJOCNemXTYs2EZ4HMNLk3m2
B/8843F1QnJP0WTTEyTDA08UJIzgoSgK/bwsBLdFybr/SguExpj7aIJH8v5Z2ycV
0tpC+Veoo4nPFEs5Zd3+g7o7QdMV/AKyZ/s8mGvEcQKBgQDQ1THa1gN9/ff7yJWc
Qrre/KO+7/KgETluwfjGYNkhWEe4PYbGO+lW0mGvZD6eslj4eBbm+lUtIHks+4YD
l2AxBeMV3h9dYIRPh7N3yFVn3aAJiK5sU7lFPcL4dOZtq+lYQSzWeYaBXOEP9LEI
ceakpJeVDFrPhKtf1v1tLj/plQKBgQDFqe+5W/UroBZG2lSgFwQ5f5BJBE9lXsTu
457TvjtST8aPP4nOAjuhT6MDbgYeP412RYjWbfvpGAHZa6xfhztGCqI2Ev0Q3/mV
oeeHX9r2sYq65BffvMEgw4gKFCiZ8xJTKzEZEEyZ0gh3jTMk4mms93ew03ViapIY
vrS3PhjYyQKBgQCKBc5cl4RZWmjzNaCEVapSxOGoycgvORMfe/5jhxEbM9C7GZch
H+nZ41SC6ptkofWhyyU/5gYzvDm6nEb3yq3d2Mk848ERI0Bvm/3m1jZ0XotuobK+
kBtsgySAuCqwI6YnGXR8EHfwuiVaOVxke3t4J/yzmyXN8B6gSmTXK3E8fQKBgDAu
fz/YmYebyzJUMAKh+aamYJ5bzZqxIiH1HBcTLNSgm475dvbfdneYuOyyGg2vgiUN
SBC02I32CyVbaLYUea9WEjpKIKPHZMhDofNOu0oc9usdhHBGS3FYGEYUqdz08keR
pLMuVO2909CIe6oHAqll3SgeM2PdBGXBvr1YBqh5AoGAY5VQ7aGeLxZuaOK+9KIu
hVQankaSDC0T1yCKS3jnK91ea3si2KDEnk99uDspH7M/tZohXVt8rXE3cykLqZMk
HZr7Rf7ndVPj6E6x41qOUwRgZtSOWbYY4tfeAcr/64E/KwE9cnvB4XIxrxrGOVwH
fVY5JLsbM7l4Egd233vN6Yo=
-----END PRIVATE KEY-----";
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
    }

    #[async_trait]
    impl OidcHttpClient for MockHttpClient {
        async fn get_json(
            &self,
            url: &str,
            _bearer_token: Option<&str>,
        ) -> Result<Value, OidcError> {
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
}
