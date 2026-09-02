use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation};
use rskit_errors::{AppError, AppResult, ErrorCode};
use serde::{Serialize, de::DeserializeOwned};

use super::config::{AsymmetricAlgorithm, JwtAlgorithm, JwtConfig, JwtKeyMaterial};

/// Header `typ` written on access tokens.
pub const ACCESS_TOKEN_TYPE: &str = "JWT";

/// Header `typ` written on refresh tokens so they cannot be replayed as access tokens.
pub const REFRESH_TOKEN_TYPE: &str = "refresh+jwt";

/// Decoded JWT header metadata exposed without leaking the underlying JWT library.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub struct JwtHeader {
    /// Header algorithm.
    pub algorithm: JwtAlgorithm,
    /// Token type, commonly `JWT`.
    pub token_type: Option<String>,
    /// Key identifier.
    pub key_id: Option<String>,
    /// Content type.
    pub content_type: Option<String>,
}

impl TryFrom<Header> for JwtHeader {
    type Error = AppError;

    fn try_from(header: Header) -> AppResult<Self> {
        Ok(Self {
            algorithm: algorithm_from_jsonwebtoken(header.alg)?,
            token_type: header.typ,
            key_id: header.kid,
            content_type: header.cty,
        })
    }
}

/// Reusable JWT codec for safe token encoding, header inspection, and validation.
///
/// This is the rskit-owned primitive used by [`JwtService`](super::JwtService).
/// Application code can use it when it needs direct JWT operations without depending on the underlying JWT library.
pub struct JwtCodec {
    config: JwtConfig,
    encoding_key: EncodingKey,
    decoding_key: DecodingKey,
    refresh_encoding_key: EncodingKey,
    refresh_decoding_key: DecodingKey,
}

impl JwtCodec {
    /// Create a codec from JWT configuration and key material.
    ///
    /// # Errors
    /// Returns an error when the issuer/audience policy or key material is invalid.
    pub fn new(config: JwtConfig) -> AppResult<Self> {
        validate_config(&config)?;
        let (encoding_key, decoding_key) = build_keys(&config.key_material)?;
        let (refresh_encoding_key, refresh_decoding_key) = build_refresh_keys(&config)?;
        Ok(Self {
            config,
            encoding_key,
            decoding_key,
            refresh_encoding_key,
            refresh_decoding_key,
        })
    }

    /// Return this codec's immutable validation/signing configuration.
    #[must_use]
    pub const fn config(&self) -> &JwtConfig {
        &self.config
    }

    /// Sign claims using the configured algorithm and key material.
    ///
    /// The token is explicitly typed as [`ACCESS_TOKEN_TYPE`] so it cannot be presented where a
    /// refresh token is expected. This method does not synthesize or mutate registered claims;
    /// callers own claim construction, while [`decode`](Self::decode) enforces the configured
    /// validation policy.
    ///
    /// # Errors
    /// Returns an error when claim serialization or signing fails.
    pub fn encode<C: Serialize>(&self, claims: &C) -> AppResult<String> {
        let mut header = Header::new(self.config.algorithm().as_jsonwebtoken());
        header.typ = Some(ACCESS_TOKEN_TYPE.to_owned());
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

    /// Decode and validate a token using the configured validation policy.
    ///
    /// The header `typ` must be exactly [`ACCESS_TOKEN_TYPE`] (allowlist), so no other token
    /// class minted from the same issuer/key — refresh tokens, or any future type — can be
    /// replayed as an access token even when they share key material. Tokens with a missing or
    /// mismatched `typ` are rejected.
    ///
    /// # Errors
    /// Returns an authentication error when the token is malformed, signed with the wrong algorithm/key,
    /// not typed as an access token, expired, immature, or missing required claims.
    pub fn decode<C: DeserializeOwned>(&self, token: &str) -> AppResult<C> {
        let header =
            jsonwebtoken::decode_header(token).map_err(|error| map_validation_error(&error))?;
        let configured_algorithm = self.config.algorithm().as_jsonwebtoken();
        if header.alg != configured_algorithm {
            return Err(AppError::invalid_token().context("JWT algorithm mismatch"));
        }
        if !is_access_type(header.typ.as_deref()) {
            return Err(AppError::invalid_token().context("token is not typed as an access token"));
        }

        let validation = validation_for(&self.config);
        let data = jsonwebtoken::decode::<C>(token, &self.decoding_key, &validation)
            .map_err(|error| map_validation_error(&error))?;
        Ok(data.claims)
    }

    /// Sign claims as a refresh token using the configured refresh key material.
    ///
    /// The token is explicitly typed as [`REFRESH_TOKEN_TYPE`], which binds its class independently
    /// of key material. When [`JwtConfig::refresh_secret`] is set (HMAC only), refresh tokens are
    /// additionally signed with that separate secret; otherwise the primary key material is reused.
    /// Callers own claim construction and may size expiry with [`JwtConfig::refresh_ttl`].
    ///
    /// # Errors
    /// Returns an error when claim serialization or signing fails.
    pub fn encode_refresh<C: Serialize>(&self, claims: &C) -> AppResult<String> {
        let mut header = Header::new(self.config.algorithm().as_jsonwebtoken());
        header.typ = Some(REFRESH_TOKEN_TYPE.to_owned());
        jsonwebtoken::encode(&header, claims, &self.refresh_encoding_key).map_err(|error| {
            AppError::new(
                ErrorCode::Internal,
                format!(
                    "JWT refresh encode error for {:?}: {error}",
                    self.config.algorithm()
                ),
            )
        })
    }

    /// Decode and validate a refresh token using the configured refresh key and validation policy.
    ///
    /// The header `typ` must be [`REFRESH_TOKEN_TYPE`], so an access token is never accepted here.
    ///
    /// # Errors
    /// Returns an authentication error when the token is malformed, signed with the wrong
    /// algorithm/key, not typed as a refresh token, expired, immature, or missing required claims.
    pub fn decode_refresh<C: DeserializeOwned>(&self, token: &str) -> AppResult<C> {
        let header =
            jsonwebtoken::decode_header(token).map_err(|error| map_validation_error(&error))?;
        let configured_algorithm = self.config.algorithm().as_jsonwebtoken();
        if header.alg != configured_algorithm {
            return Err(AppError::invalid_token().context("JWT algorithm mismatch"));
        }
        if !is_refresh_type(header.typ.as_deref()) {
            return Err(AppError::invalid_token().context("token is not typed as a refresh token"));
        }

        let validation = validation_for(&self.config);
        let data = jsonwebtoken::decode::<C>(token, &self.refresh_decoding_key, &validation)
            .map_err(|error| map_validation_error(&error))?;
        Ok(data.claims)
    }

    /// Decode JWT header metadata without validating the token signature.
    ///
    /// # Errors
    /// Returns an error when the token header is malformed or uses an unsupported algorithm.
    pub fn decode_header(token: &str) -> AppResult<JwtHeader> {
        jsonwebtoken::decode_header(token)
            .map_err(|error| map_validation_error(&error))
            .and_then(JwtHeader::try_from)
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
    if config
        .audience
        .iter()
        .any(|audience| audience.trim().is_empty())
    {
        return Err(AppError::invalid_input(
            "audience",
            "audience values must not be empty",
        ));
    }
    if config.leeway.as_secs() > 60 {
        return Err(AppError::invalid_input(
            "leeway",
            "clock skew tolerance must be 60 seconds or less",
        ));
    }
    if config.algorithm().is_symmetric() && !config.allow_symmetric_hmac {
        return Err(AppError::invalid_input(
            "allow_symmetric_hmac",
            "HS256 requires allow_symmetric_hmac and is intended for internal-only deployments",
        ));
    }
    if config.refresh_secret.is_some() && !config.algorithm().is_symmetric() {
        return Err(AppError::invalid_input(
            "refresh_secret",
            "refresh_secret is only valid for the symmetric HMAC algorithm",
        ));
    }
    Ok(())
}

fn build_keys(key_material: &JwtKeyMaterial) -> AppResult<(EncodingKey, DecodingKey)> {
    match key_material {
        JwtKeyMaterial::Hmac { secret } => build_hmac_keys(secret),
        JwtKeyMaterial::Asymmetric { algorithm, keys } => {
            let priv_pem = keys.private_key_pem.expose().as_bytes();
            let pub_pem = keys.public_key_pem.expose().as_bytes();
            let (enc, dec) = match algorithm {
                AsymmetricAlgorithm::Rs256 => (
                    EncodingKey::from_rsa_pem(priv_pem),
                    DecodingKey::from_rsa_pem(pub_pem),
                ),
                AsymmetricAlgorithm::Es256 => (
                    EncodingKey::from_ec_pem(priv_pem),
                    DecodingKey::from_ec_pem(pub_pem),
                ),
                AsymmetricAlgorithm::EdDsa => (
                    EncodingKey::from_ed_pem(priv_pem),
                    DecodingKey::from_ed_pem(pub_pem),
                ),
            };
            Ok((
                enc.map_err(|e| jwt_key_error(&e))?,
                dec.map_err(|e| jwt_key_error(&e))?,
            ))
        }
    }
}

fn build_hmac_keys(secret: &rskit_util::SecretString) -> AppResult<(EncodingKey, DecodingKey)> {
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

/// Build the refresh signing/verification keys.
///
/// For HMAC key material with a configured [`JwtConfig::refresh_secret`], a separate HMAC key pair
/// is derived from that secret; otherwise the primary key material is reused.
fn build_refresh_keys(config: &JwtConfig) -> AppResult<(EncodingKey, DecodingKey)> {
    match (&config.key_material, &config.refresh_secret) {
        (JwtKeyMaterial::Hmac { .. }, Some(refresh_secret)) => build_hmac_keys(refresh_secret),
        _ => build_keys(&config.key_material),
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

/// Report whether a header `typ` marks the token as a refresh token.
///
/// Media types are case-insensitive, so the comparison ignores case.
fn is_refresh_type(token_type: Option<&str>) -> bool {
    token_type.is_some_and(|value| value.eq_ignore_ascii_case(REFRESH_TOKEN_TYPE))
}

/// Report whether a header `typ` marks the token as an access token.
///
/// This is an allowlist: only the exact [`ACCESS_TOKEN_TYPE`] is accepted, so tokens with a
/// missing or foreign `typ` are rejected rather than defaulting to access-token treatment.
/// Media types are case-insensitive, so the comparison ignores case.
fn is_access_type(token_type: Option<&str>) -> bool {
    token_type.is_some_and(|value| value.eq_ignore_ascii_case(ACCESS_TOKEN_TYPE))
}

fn algorithm_from_jsonwebtoken(algorithm: jsonwebtoken::Algorithm) -> AppResult<JwtAlgorithm> {
    match algorithm {
        jsonwebtoken::Algorithm::HS256 => Ok(JwtAlgorithm::Hs256),
        jsonwebtoken::Algorithm::RS256 => Ok(JwtAlgorithm::Rs256),
        jsonwebtoken::Algorithm::ES256 => Ok(JwtAlgorithm::Es256),
        jsonwebtoken::Algorithm::EdDSA => Ok(JwtAlgorithm::EdDsa),
        _ => Err(AppError::invalid_token().context("JWT algorithm is not allowed by rskit")),
    }
}

#[cfg(test)]
mod tests {
    use serde::{Deserialize, Serialize};

    use super::*;

    const ISSUER: &str = "https://issuer.example";
    const AUDIENCE: &str = "rskit-tests";

    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
    struct Claims {
        sub: String,
        iss: String,
        aud: Vec<String>,
        exp: u64,
        nbf: u64,
        iat: u64,
    }

    fn now() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
    }

    fn claims() -> Claims {
        let now = now();
        Claims {
            sub: "user-1".to_owned(),
            iss: ISSUER.to_owned(),
            aud: vec![AUDIENCE.to_owned()],
            exp: now + 3600,
            nbf: now.saturating_sub(1),
            iat: now,
        }
    }

    fn codec() -> JwtCodec {
        JwtCodec::new(JwtConfig::hmac(
            "codec-test-secret-32-bytes-long!",
            ISSUER,
            vec![AUDIENCE.to_owned()],
        ))
        .unwrap()
    }

    #[test]
    fn codec_roundtrip_and_header_decode_use_rskit_types() {
        let codec = codec();
        let token = codec.encode(&claims()).unwrap();

        let header = JwtCodec::decode_header(&token).unwrap();
        assert_eq!(header.algorithm, JwtAlgorithm::Hs256);
        assert_eq!(header.token_type.as_deref(), Some("JWT"));

        let decoded: Claims = codec.decode(&token).unwrap();
        assert_eq!(decoded.sub, "user-1");
    }

    #[test]
    fn refresh_tokens_roundtrip_and_stay_isolated_with_separate_secret() {
        let codec = JwtCodec::new(
            JwtConfig::hmac(
                "codec-test-secret-32-bytes-long!",
                ISSUER,
                vec![AUDIENCE.to_owned()],
            )
            .with_refresh_secret("codec-refresh-secret-32-bytes-long!!"),
        )
        .unwrap();

        let refresh = codec.encode_refresh(&claims()).unwrap();
        let decoded: Claims = codec.decode_refresh(&refresh).unwrap();
        assert_eq!(decoded.sub, "user-1");

        // A refresh token signed with the separate secret must not validate as an access token.
        assert!(codec.decode::<Claims>(&refresh).is_err());
        // ...and an access token must not validate as a refresh token.
        let access = codec.encode(&claims()).unwrap();
        assert!(codec.decode_refresh::<Claims>(&access).is_err());
    }

    #[test]
    fn refresh_tokens_stay_isolated_when_reusing_primary_key() {
        let codec = codec();

        let refresh = codec.encode_refresh(&claims()).unwrap();
        let decoded: Claims = codec.decode_refresh(&refresh).unwrap();
        assert_eq!(decoded.sub, "user-1");

        // Shared key material must not make the two token classes interchangeable.
        assert_eq!(
            codec.decode::<Claims>(&refresh).unwrap_err().code(),
            ErrorCode::InvalidToken
        );
        let access = codec.encode(&claims()).unwrap();
        assert_eq!(
            codec.decode_refresh::<Claims>(&access).unwrap_err().code(),
            ErrorCode::InvalidToken
        );
    }

    #[test]
    fn refresh_tokens_carry_explicit_header_type() {
        let codec = codec();

        let access = JwtCodec::decode_header(&codec.encode(&claims()).unwrap()).unwrap();
        assert_eq!(access.token_type.as_deref(), Some(ACCESS_TOKEN_TYPE));

        let refresh = JwtCodec::decode_header(&codec.encode_refresh(&claims()).unwrap()).unwrap();
        assert_eq!(refresh.token_type.as_deref(), Some(REFRESH_TOKEN_TYPE));
    }

    #[test]
    fn codec_rejects_symmetric_hmac_without_explicit_opt_in() {
        let config = JwtConfig {
            allow_symmetric_hmac: false,
            ..JwtConfig::hmac(
                "codec-test-secret-32-bytes-long!",
                ISSUER,
                vec![AUDIENCE.to_owned()],
            )
        };
        let err = JwtCodec::new(config).err().unwrap();
        assert_eq!(err.code(), ErrorCode::InvalidInput);
    }

    #[test]
    fn codec_rejects_refresh_secret_for_asymmetric_material() {
        let config = JwtConfig {
            refresh_secret: Some(rskit_util::SecretString::new(
                "codec-refresh-secret-32-bytes-long!!",
            )),
            ..JwtConfig::rs256("priv", "pub", ISSUER, vec![AUDIENCE.to_owned()])
        };
        let err = JwtCodec::new(config).err().unwrap();
        assert_eq!(err.code(), ErrorCode::InvalidInput);
    }

    #[test]
    fn codec_decode_rejects_missing_required_claims() {
        let codec = codec();
        let mut claims = serde_json::json!({
            "sub": "user-1",
            "iss": ISSUER,
            "aud": [AUDIENCE],
            "exp": now() + 3600,
            "nbf": now().saturating_sub(1),
            "iat": now(),
        });
        claims.as_object_mut().unwrap().remove("aud");
        let token = codec.encode(&claims).unwrap();

        let result = codec.decode::<serde_json::Value>(&token);

        assert!(result.is_err());
    }

    #[test]
    fn codec_rejects_blank_audience_values() {
        let err = JwtCodec::new(JwtConfig::hmac(
            "codec-test-secret-32-bytes-long!",
            ISSUER,
            vec![AUDIENCE.to_owned(), " \t ".to_owned()],
        ))
        .err()
        .unwrap();

        assert_eq!(err.code(), ErrorCode::InvalidInput);
    }

    #[test]
    fn codec_rejects_invalid_policy_and_key_material() {
        for config in [
            JwtConfig::hmac(
                "codec-test-secret-32-bytes-long!",
                " \t ",
                vec![AUDIENCE.to_owned()],
            ),
            JwtConfig::hmac("codec-test-secret-32-bytes-long!", ISSUER, Vec::new()),
            JwtConfig {
                leeway: std::time::Duration::from_secs(61),
                ..JwtConfig::hmac(
                    "codec-test-secret-32-bytes-long!",
                    ISSUER,
                    vec![AUDIENCE.to_owned()],
                )
            },
            JwtConfig::hmac("", ISSUER, vec![AUDIENCE.to_owned()]),
            JwtConfig::hmac("too-short", ISSUER, vec![AUDIENCE.to_owned()]),
        ] {
            assert_eq!(
                JwtCodec::new(config).err().unwrap().code(),
                ErrorCode::InvalidInput
            );
        }

        let asymmetric = JwtConfig::rs256(
            "not a private key",
            "not a public key",
            ISSUER,
            vec![AUDIENCE.to_owned()],
        );
        assert_eq!(
            JwtCodec::new(asymmetric).err().unwrap().code(),
            ErrorCode::InvalidInput
        );
    }

    #[test]
    fn codec_rejects_algorithm_mismatch_and_malformed_header() {
        let codec = codec();
        let mut header = Header::new(jsonwebtoken::Algorithm::HS384);
        header.typ = Some("JWT".to_string());
        let token = jsonwebtoken::encode(
            &header,
            &claims(),
            &EncodingKey::from_secret(b"codec-test-secret-32-bytes-long!"),
        )
        .unwrap();

        assert_eq!(
            codec.decode::<Claims>(&token).unwrap_err().code(),
            ErrorCode::InvalidToken
        );
        assert_eq!(
            JwtCodec::decode_header("not-a-jwt").unwrap_err().code(),
            ErrorCode::InvalidToken
        );
    }

    #[test]
    fn codec_decode_requires_exact_access_token_type() {
        let codec = codec();
        let secret = b"codec-test-secret-32-bytes-long!";

        // A token with a foreign `typ` from the same issuer/key must not validate as an access token.
        let mut foreign = Header::new(jsonwebtoken::Algorithm::HS256);
        foreign.typ = Some("at+jwt".to_owned());
        let foreign_token =
            jsonwebtoken::encode(&foreign, &claims(), &EncodingKey::from_secret(secret)).unwrap();
        assert_eq!(
            codec.decode::<Claims>(&foreign_token).unwrap_err().code(),
            ErrorCode::InvalidToken
        );

        // A token missing `typ` must also be rejected (allowlist, not denylist).
        let mut missing = Header::new(jsonwebtoken::Algorithm::HS256);
        missing.typ = None;
        let missing_token =
            jsonwebtoken::encode(&missing, &claims(), &EncodingKey::from_secret(secret)).unwrap();
        assert_eq!(
            codec.decode::<Claims>(&missing_token).unwrap_err().code(),
            ErrorCode::InvalidToken
        );

        // The refresh token class stays rejected as before.
        let refresh = codec.encode_refresh(&claims()).unwrap();
        assert_eq!(
            codec.decode::<Claims>(&refresh).unwrap_err().code(),
            ErrorCode::InvalidToken
        );

        // The canonical access token still validates.
        let access = codec.encode(&claims()).unwrap();
        assert!(codec.decode::<Claims>(&access).is_ok());
    }
}
