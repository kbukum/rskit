use jsonwebtoken::{DecodingKey, EncodingKey, Header, Validation};
use rskit_errors::{AppError, AppResult, ErrorCode};
use serde::{Serialize, de::DeserializeOwned};

use super::config::{AsymmetricAlgorithm, JwtAlgorithm, JwtConfig, JwtKeyMaterial};

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
/// Application code can use it when it needs direct JWT operations without
/// depending on the underlying JWT library.
pub struct JwtCodec {
    config: JwtConfig,
    encoding_key: EncodingKey,
    decoding_key: DecodingKey,
}

impl JwtCodec {
    /// Create a codec from JWT configuration and key material.
    ///
    /// # Errors
    /// Returns an error when the issuer/audience policy or key material is invalid.
    pub fn new(config: JwtConfig) -> AppResult<Self> {
        validate_config(&config)?;
        let (encoding_key, decoding_key) = build_keys(&config.key_material)?;
        Ok(Self {
            config,
            encoding_key,
            decoding_key,
        })
    }

    /// Return this codec's immutable validation/signing configuration.
    #[must_use]
    pub const fn config(&self) -> &JwtConfig {
        &self.config
    }

    /// Sign claims using the configured algorithm and key material.
    ///
    /// This method does not synthesize or mutate registered claims; callers own
    /// claim construction, while [`decode`](Self::decode) enforces the configured
    /// validation policy.
    ///
    /// # Errors
    /// Returns an error when claim serialization or signing fails.
    pub fn encode<C: Serialize>(&self, claims: &C) -> AppResult<String> {
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

    /// Decode and validate a token using the configured validation policy.
    ///
    /// # Errors
    /// Returns an authentication error when the token is malformed, signed with
    /// the wrong algorithm/key, expired, immature, or missing required claims.
    pub fn decode<C: DeserializeOwned>(&self, token: &str) -> AppResult<C> {
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

    /// Decode JWT header metadata without validating the token signature.
    ///
    /// # Errors
    /// Returns an error when the token header is malformed or uses an unsupported
    /// algorithm.
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

fn algorithm_from_jsonwebtoken(algorithm: jsonwebtoken::Algorithm) -> AppResult<JwtAlgorithm> {
    match algorithm {
        jsonwebtoken::Algorithm::HS256 => Ok(JwtAlgorithm::Hs256Internal),
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
        JwtCodec::new(JwtConfig::hs256_internal(
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
        assert_eq!(header.algorithm, JwtAlgorithm::Hs256Internal);
        assert_eq!(header.token_type.as_deref(), Some("JWT"));

        let decoded: Claims = codec.decode(&token).unwrap();
        assert_eq!(decoded.sub, "user-1");
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
        let err = JwtCodec::new(JwtConfig::hs256_internal(
            "codec-test-secret-32-bytes-long!",
            ISSUER,
            vec![AUDIENCE.to_owned(), " \t ".to_owned()],
        ))
        .err()
        .unwrap();

        assert_eq!(err.code(), ErrorCode::InvalidInput);
    }
}
