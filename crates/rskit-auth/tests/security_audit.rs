//! Security audit tests for rskit-auth and rskit-errors.

mod common;

use common::{AUDIENCE, ISSUER, StandardClaims, jwt_service, now_epoch, standard_config};
use jsonwebtoken::{Algorithm, EncodingKey, Header, encode};
use rskit_auth::jwt::{JwtConfig, JwtService};
use rskit_auth::password::{HashAlgorithm, PasswordHasher, ResetTokenGenerator};
use rskit_auth::traits::{TokenGenerator, TokenValidator};
use rskit_errors::{AppError, ErrorCode, ProblemDetail};

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
const RSA_PUBLIC_KEY: &str = r"-----BEGIN PUBLIC KEY-----
MIIBIjANBgkqhkiG9w0BAQEFAAOCAQ8AMIIBCgKCAQEAoT6vqY7IFYxu8LYnCgPs
GZICOgRF57XQI9wAoILWbGwIBTzZMi/KuWWtYCo+Ph3LhN2vsIUQkKwT1+kob3If
BoCh9kN0iO6f/XLXgAOCCtVyrt5bjLyFJvtYcFecfh80LvdEeL8VE7Fxnd6CoRv1
zNszWFxsfSCeWPyWQefJjLcWOqRg5zyFuP5yHwGwzEI3wLKhPFC2ufTupu4GRcFp
jKpM3ZxoWw2BEaPcSmKMFWBGd7lsKaBe6TfLvL3pDwzBHSngURGInXrTQGmw+KRp
Drcv5vlUD7QwVWP3JWGW66RtWH5qS5MP2eeOOSd48/Yccbx1kbGZ+NQtk6fSOTwd
/QIDAQAB
-----END PUBLIC KEY-----";

// ─── 1. Error Display Doesn't Leak Secrets ─────────────────────────────────

#[test]
fn error_display_does_not_leak_sensitive_data() {
    let secrets = [
        "super-secret-api-key-12345",
        "postgres://admin:s3cret@db:5432",
        "-----BEGIN RSA PRIVATE KEY-----",
    ];

    let errors = vec![
        AppError::unauthorized("Authentication required."),
        AppError::forbidden("Access denied."),
        AppError::token_expired(),
        AppError::invalid_token(),
        AppError::internal(std::io::Error::new(
            std::io::ErrorKind::ConnectionRefused,
            "refused",
        )),
    ];

    for err in &errors {
        let display = format!("{err}");
        for secret in &secrets {
            assert!(
                !display.contains(secret),
                "Error display leaked secret {secret:?}: {display}"
            );
        }
    }
}

#[test]
fn internal_error_message_is_generic_when_displayed_via_error_response() {
    let cause = std::io::Error::new(
        std::io::ErrorKind::PermissionDenied,
        "password=s3cret host=db.internal:5432",
    );
    let err = AppError::internal(cause);
    let response = ProblemDetail::from(&err);
    assert!(
        !response.detail.contains("s3cret"),
        "ProblemDetail leaked secret: {:?}",
        response.detail
    );
}

#[test]
fn auth_error_messages_do_not_reveal_system_info() {
    let auth_errors = vec![
        AppError::unauthorized("bad token"),
        AppError::forbidden("no access"),
        AppError::token_expired(),
        AppError::invalid_token(),
    ];

    let forbidden_patterns = ["/usr/", "/etc/", "/var/", "127.0.0.1", "goroutine"];

    for err in &auth_errors {
        let display = format!("{err}");
        for pattern in &forbidden_patterns {
            assert!(
                !display.to_lowercase().contains(&pattern.to_lowercase()),
                "Auth error leaked system info {pattern:?}: {display}"
            );
        }
    }
}

// ─── 2. JWT Algorithm Verification ─────────────────────────────────────────

#[tokio::test]
async fn jwt_wrong_secret_rejected() {
    let svc1 = jwt_service("secret-key-one-for-service-a");
    let svc2 = jwt_service("secret-key-two-for-service-b");

    let claims = StandardClaims::new("user-123");
    let token = svc1.generate(&claims).await.unwrap();
    let result = svc2.validate(&token).await;
    assert!(
        result.is_err(),
        "Token signed with different secret should be rejected"
    );
}

#[tokio::test]
async fn jwt_expired_token_rejected() {
    let svc = jwt_service("test-secret-key-for-audit");
    let mut claims = StandardClaims::new("user-123");
    claims.exp = 1;

    let token = svc.generate(&claims).await.unwrap();
    let result = svc.validate(&token).await;
    assert!(result.is_err(), "Expired token should be rejected");

    let err = result.unwrap_err();
    assert_eq!(err.code, ErrorCode::TokenExpired);
}

#[tokio::test]
async fn jwt_malformed_tokens_rejected() {
    let svc = jwt_service("test-secret-key-for-audit");

    let long_token = "a".repeat(100_000);
    let malformed_tokens = vec!["", "not-a-jwt", "a.b", "a.b.c", "   ", &long_token];

    for token in malformed_tokens {
        let result = svc.validate(token).await;
        assert!(
            result.is_err(),
            "Malformed token should be rejected: {:?}",
            &token[..token.len().min(20)]
        );
    }
}

#[tokio::test]
async fn jwt_parse_error_does_not_leak_secret() {
    let secret = "ultra-secret-key-that-must-not-leak";
    let svc = JwtService::<StandardClaims>::new(standard_config(secret)).unwrap();

    let result = svc.validate("invalid.token.here").await;
    assert!(result.is_err());

    let err = result.unwrap_err();
    let display = format!("{err}");
    assert!(
        !display.contains(secret),
        "JWT error leaked secret: {display}"
    );
}

#[tokio::test]
async fn jwt_roundtrip_succeeds() {
    let svc = jwt_service("test-secret-key-for-audit");
    let claims = StandardClaims::new("user-123");

    let token = svc.generate(&claims).await.unwrap();
    let decoded = svc.validate(&token).await.unwrap();
    assert_eq!(decoded, claims);
}

#[tokio::test]
async fn jwt_algorithm_confusion_attack_is_rejected() {
    let validator = JwtService::<StandardClaims>::new(JwtConfig::rs256(
        RSA_PRIVATE_KEY,
        RSA_PUBLIC_KEY,
        ISSUER,
        vec![AUDIENCE.into()],
    ))
    .unwrap();

    let claims = StandardClaims::new("user-123");
    let header = Header::new(Algorithm::HS256);
    let attack_token = encode(
        &header,
        &claims,
        &EncodingKey::from_secret(RSA_PUBLIC_KEY.as_bytes()),
    )
    .unwrap();

    let result = validator.validate(&attack_token).await;
    assert!(
        result.is_err(),
        "algorithm confusion token must be rejected"
    );
}

#[tokio::test]
async fn jwt_alg_none_is_rejected() {
    let validator = jwt_service("alg-none-test");
    let token = concat!(
        "eyJhbGciOiJub25lIiwidHlwIjoiSldUIn0.",
        "eyJzdWIiOiJ1c2VyLTEyMyIsImlzcyI6Imh0dHBzOi8vaXNzdWVyLnJza2l0LnRlc3QiLCJhdWQiOlsicnNraXQtYXV0aC10ZXN0cyJdLCJleHAiOjQxMDI0NDQ4MDAsIm5iZiI6MTcwMDAwMDAwMCwiaWF0IjoxNzAwMDAwMDAwfQ.",
        ""
    );

    assert!(validator.validate(token).await.is_err());
}

// ─── 3. Password Hashing Safety ────────────────────────────────────────────

#[test]
fn argon2_hash_uses_argon2id() {
    let hasher = PasswordHasher::default();
    let hash = hasher.hash("hunter2hunter2").unwrap();
    assert!(
        hash.contains("$argon2id$"),
        "Hash should use argon2id variant: {hash}"
    );
}

#[test]
fn argon2_hash_verify_roundtrip() {
    let hasher = PasswordHasher::default();
    let hash = hasher.hash("securepassword1").unwrap();
    assert!(hasher.verify("securepassword1", &hash).unwrap());
    assert!(!hasher.verify("wrong-password1", &hash).unwrap());
}

#[test]
fn argon2_same_password_different_hashes() {
    let hasher = PasswordHasher::default();
    let h1 = hasher.hash("securepassword1").unwrap();
    let h2 = hasher.hash("securepassword1").unwrap();
    assert_ne!(
        h1, h2,
        "Same password should produce different hashes (random salt)"
    );
}

#[test]
fn argon2_malformed_hash_returns_error() {
    let hasher = PasswordHasher::default();
    let result = hasher.verify("password", "not-a-valid-hash");
    assert!(result.is_err(), "Malformed hash should return error");
}

#[test]
fn argon2_empty_password_hashes_without_panic() {
    let hasher = PasswordHasher::default();
    let _ = hasher.hash("");
}

#[test]
fn argon2_unicode_password_roundtrip() {
    let hasher = PasswordHasher::default();
    let passwords = ["pässwörd-ünïcödé", "密码测试密码测试", "p@$$w0rd!#%^&*()"];
    for pw in &passwords {
        let hash = hasher.hash(pw).unwrap();
        assert!(
            hasher.verify(pw, &hash).unwrap(),
            "Unicode password verify failed: {pw}"
        );
    }
}

// ─── 4. Reset Token Security ───────────────────────────────────────────────

#[test]
fn reset_tokens_are_unique() {
    let generator = ResetTokenGenerator::new(std::time::Duration::from_mins(5));
    let (t1, _) = generator.generate();
    let (t2, _) = generator.generate();
    assert_ne!(t1, t2, "Reset tokens should be unique");
}

#[test]
fn reset_token_has_sufficient_entropy() {
    let generator = ResetTokenGenerator::new(std::time::Duration::from_mins(5));
    let (token, _) = generator.generate();
    assert!(
        token.len() >= 40,
        "Reset token should have sufficient length (got {})",
        token.len()
    );
}

#[test]
fn reset_token_expiry_is_in_future() {
    let generator = ResetTokenGenerator::new(std::time::Duration::from_mins(5));
    let (_, exp) = generator.generate();
    assert!(
        exp > chrono::Utc::now(),
        "Reset token expiry should be in the future"
    );
}

// ─── 5. Send+Sync Bounds ──────────────────────────────────────────────────

#[test]
fn public_types_are_send_and_sync() {
    fn assert_send_sync<T: Send + Sync>() {}

    assert_send_sync::<AppError>();
    assert_send_sync::<ErrorCode>();
    assert_send_sync::<ProblemDetail>();
    assert_send_sync::<JwtConfig>();
    assert_send_sync::<JwtService<StandardClaims>>();
    assert_send_sync::<PasswordHasher>();
    assert_send_sync::<HashAlgorithm>();
    assert_send_sync::<ResetTokenGenerator>();
}

// ─── 6. Problem Detail (RFC 9457) Safety ──────────────────────────────────

#[test]
fn problem_detail_from_app_error_has_correct_status() {
    let cases = vec![
        (AppError::unauthorized("no"), 401u16),
        (AppError::forbidden("no"), 403),
        (AppError::not_found("User", None), 404),
        (AppError::token_expired(), 401),
        (AppError::invalid_token(), 401),
        (AppError::rate_limited(), 429),
    ];

    for (err, expected_status) in cases {
        let pd = ProblemDetail::from(&err);
        assert_eq!(
            pd.status, expected_status,
            "Wrong status for {:?}: got {}",
            err.code, pd.status
        );
    }
}

#[test]
fn error_response_serialization_excludes_cause() {
    let err = AppError::internal(std::io::Error::other("secret connection string here"));
    let json = serde_json::to_string(&err).unwrap();
    assert!(!json.is_empty());
    assert!(
        !json.contains("secret connection string here"),
        "Serialized error leaked internal cause: {json}"
    );
}

// ─── 7. Concurrent JWT Operations ──────────────────────────────────────────

#[tokio::test]
async fn concurrent_jwt_generate_and_validate() {
    let svc = std::sync::Arc::new(jwt_service("test-secret-key-for-audit"));

    let mut handles = Vec::new();
    for i in 0..20 {
        let svc = svc.clone();
        handles.push(tokio::spawn(async move {
            let mut claims = StandardClaims::new(format!("user-{i}"));
            claims.iat = now_epoch();
            let token = svc.generate(&claims).await.unwrap();
            let decoded = svc.validate(&token).await.unwrap();
            assert_eq!(decoded.sub, format!("user-{i}"));
        }));
    }

    for h in handles {
        h.await.unwrap();
    }
}
