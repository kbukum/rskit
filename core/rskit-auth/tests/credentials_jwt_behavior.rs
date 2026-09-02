//! Behavioral tests for JWT credentials, password hashing, and token helpers.

#![allow(missing_docs, clippy::missing_const_for_fn)]

mod common;

use std::time::Duration;

use common::{
    AUDIENCE, ISSUER, StandardClaims, future_exp, jwt_service, now_epoch, standard_config,
};
use rskit_auth::{
    JwtCodec, JwtConfig, JwtService, PasswordHasher, ResetTokenGenerator, TokenGenerator,
    TokenValidator,
};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct TenantClaims {
    sub: String,
    iss: String,
    aud: Vec<String>,
    exp: u64,
    nbf: u64,
    iat: u64,
    tenant: String,
}

impl TenantClaims {
    fn new(sub: &str, tenant: &str) -> Self {
        let now = now_epoch();
        Self {
            sub: sub.into(),
            iss: ISSUER.into(),
            aud: vec![AUDIENCE.into()],
            exp: future_exp(),
            nbf: now.saturating_sub(1),
            iat: now,
            tenant: tenant.into(),
        }
    }
}

// ══════════════════════════════════════════════════════════════════════
// JWT — Generate & Validate
// ══════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn jwt_roundtrip_preserves_claims() {
    let svc = jwt_service("test-secret-key-for-auth");
    let claims = StandardClaims::new("user-abc");
    let token = svc.generate(&claims).await.unwrap();
    let decoded = svc.validate(&token).await.unwrap();
    assert_eq!(decoded.sub, "user-abc");
}

#[tokio::test]
async fn jwt_expired_token_rejected() {
    let svc = jwt_service("test-secret-key-for-auth");
    let mut claims = StandardClaims::new("user-1");
    claims.exp = 1;
    let token = svc.generate(&claims).await.unwrap();
    assert!(svc.validate(&token).await.is_err());
}

#[tokio::test]
async fn jwt_invalid_signature_rejected() {
    let svc1 = jwt_service("secret-one");
    let svc2 = jwt_service("secret-two");
    let claims = StandardClaims::new("u");
    let token = svc1.generate(&claims).await.unwrap();
    assert!(svc2.validate(&token).await.is_err());
}

#[tokio::test]
async fn jwt_empty_token_rejected() {
    let svc = jwt_service("test-secret-key-for-auth");
    assert!(svc.validate("").await.is_err());
}

#[tokio::test]
async fn jwt_garbage_token_rejected() {
    let svc = jwt_service("test-secret-key-for-auth");
    assert!(svc.validate("not.a.jwt").await.is_err());
}

#[tokio::test]
async fn jwt_no_dots_rejected() {
    let svc = jwt_service("test-secret-key-for-auth");
    assert!(svc.validate("nodots").await.is_err());
}

#[tokio::test]
async fn jwt_issuer_validation() {
    let svc_gen = JwtService::<StandardClaims>::new(standard_config("shared")).unwrap();
    let svc_val = JwtService::<StandardClaims>::new(JwtConfig::hmac(
        "shared--padded-to-32-bytes------",
        "https://other-issuer.test",
        vec![AUDIENCE.into()],
    ))
    .unwrap();
    let claims = StandardClaims::new("u");
    let token = svc_gen.generate(&claims).await.unwrap();
    assert!(svc_val.validate(&token).await.is_err());
}

#[tokio::test]
async fn jwt_audience_validation() {
    let svc_gen = JwtService::<StandardClaims>::new(standard_config("shared")).unwrap();
    let svc_val = JwtService::<StandardClaims>::new(JwtConfig::hmac(
        "shared--padded-to-32-bytes------",
        ISSUER,
        vec!["other-audience".into()],
    ))
    .unwrap();
    let claims = StandardClaims::new("u");
    let token = svc_gen.generate(&claims).await.unwrap();
    assert!(svc_val.validate(&token).await.is_err());
}

#[tokio::test]
async fn jwt_large_claims_roundtrip() {
    let svc = jwt_service("test-secret-key-for-auth");
    let mut claims = StandardClaims::new("x".repeat(10_000));
    claims.sub = "x".repeat(10_000);
    let token = svc.generate(&claims).await.unwrap();
    let decoded = svc.validate(&token).await.unwrap();
    assert_eq!(decoded.sub.len(), 10_000);
}

#[tokio::test]
async fn jwt_special_chars_in_claims() {
    let svc = jwt_service("test-secret-key-for-auth");
    let claims = StandardClaims::new("用户/special<chars>&\"quotes\"");
    let token = svc.generate(&claims).await.unwrap();
    let decoded = svc.validate(&token).await.unwrap();
    assert_eq!(decoded.sub, claims.sub);
}

#[tokio::test]
async fn jwt_concurrent_validation() {
    let claims = StandardClaims::new("user-concurrent");
    let token = jwt_service("test-secret-key-for-auth")
        .generate(&claims)
        .await
        .unwrap();

    let mut handles = vec![];
    for _ in 0..20 {
        let svc_clone = jwt_service("test-secret-key-for-auth");
        let t = token.clone();
        handles.push(tokio::spawn(async move {
            let decoded = svc_clone.validate(&t).await.unwrap();
            assert_eq!(decoded.sub, "user-concurrent");
        }));
    }
    for h in handles {
        h.await.unwrap();
    }
}

#[tokio::test]
async fn jwt_requires_iat_and_nbf_claims() {
    #[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
    struct MissingClaims {
        sub: String,
        iss: String,
        aud: Vec<String>,
        exp: u64,
    }

    let svc = JwtService::<MissingClaims>::new(standard_config("shared")).unwrap();
    let claims = MissingClaims {
        sub: "u".into(),
        iss: ISSUER.into(),
        aud: vec![AUDIENCE.into()],
        exp: future_exp(),
    };
    let token = svc.generate(&claims).await.unwrap();
    assert!(svc.validate(&token).await.is_err());
}

#[tokio::test]
async fn jwt_requires_security_critical_registered_claims() {
    let secret = "required-claims-secret-32-bytes!!";
    let svc = JwtService::<serde_json::Value>::new(JwtConfig::hmac(
        secret,
        ISSUER,
        vec![AUDIENCE.into()],
    ))
    .unwrap();
    let codec = JwtCodec::new(JwtConfig::hmac(secret, ISSUER, vec![AUDIENCE.into()])).unwrap();
    let now = now_epoch();
    let base = serde_json::json!({
        "sub": "user-1",
        "iss": ISSUER,
        "aud": [AUDIENCE],
        "exp": future_exp(),
        "nbf": now.saturating_sub(1),
        "iat": now,
    });

    for required in ["sub", "iss", "aud", "exp"] {
        let mut claims = base.as_object().unwrap().clone();
        claims.remove(required);
        let token = codec.encode(&serde_json::Value::Object(claims)).unwrap();

        assert!(
            svc.validate(&token).await.is_err(),
            "token missing {required} must be rejected"
        );
    }
}

#[tokio::test]
async fn jwt_custom_claims_survive_validation() {
    let svc = JwtService::<TenantClaims>::new(standard_config("tenant-secret")).unwrap();
    let claims = TenantClaims::new("user-abc", "tenant-a");
    let token = svc.generate(&claims).await.unwrap();
    let decoded = svc.validate(&token).await.unwrap();
    assert_eq!(decoded.tenant, "tenant-a");
}

// ══════════════════════════════════════════════════════════════════════
// Password Hasher
// ══════════════════════════════════════════════════════════════════════

#[test]
fn password_hash_verify_roundtrip() {
    let h = PasswordHasher::default();
    let hash = h.hash("my-secure-password").unwrap();
    assert!(h.verify("my-secure-password", &hash).unwrap());
}

#[test]
fn password_wrong_password_fails() {
    let h = PasswordHasher::default();
    let hash = h.hash("correct").unwrap();
    assert!(!h.verify("wrong", &hash).unwrap());
}

#[test]
fn password_empty_string() {
    let h = PasswordHasher::default();
    let hash = h.hash("").unwrap();
    assert!(h.verify("", &hash).unwrap());
    assert!(!h.verify("notempty", &hash).unwrap());
}

#[test]
fn password_unicode() {
    let h = PasswordHasher::default();
    let pw = "пароль-密码-🔑";
    let hash = h.hash(pw).unwrap();
    assert!(h.verify(pw, &hash).unwrap());
}

#[test]
fn password_very_long() {
    let h = PasswordHasher::default();
    let pw = "a".repeat(1000);
    let hash = h.hash(&pw).unwrap();
    assert!(h.verify(&pw, &hash).unwrap());
}

#[test]
fn password_unique_salts() {
    let h = PasswordHasher::default();
    let h1 = h.hash("same-password").unwrap();
    let h2 = h.hash("same-password").unwrap();
    assert_ne!(h1, h2, "same password should produce different hashes");
}

#[test]
fn password_invalid_hash_format() {
    let h = PasswordHasher::default();
    let result = h.verify("password", "not-a-valid-hash");
    assert!(result.is_err());
}

#[test]
fn password_empty_hash_format() {
    let h = PasswordHasher::default();
    let result = h.verify("password", "");
    assert!(result.is_err());
}

#[test]
fn password_hash_starts_with_argon2() {
    let h = PasswordHasher::default();
    let hash = h.hash("test").unwrap();
    assert!(
        hash.starts_with("$argon2"),
        "default hash should be argon2id format, got: {}",
        &hash[..hash.len().min(20)]
    );
}

#[test]
fn password_concurrent_hashing() {
    use std::thread;

    let handles: Vec<_> = (0..10)
        .map(|i| {
            thread::spawn(move || {
                let h = PasswordHasher::default();
                let pw = format!("password-{i}");
                let hash = h.hash(&pw).unwrap();
                assert!(h.verify(&pw, &hash).unwrap());
            })
        })
        .collect();

    for h in handles {
        h.join().unwrap();
    }
}

// ══════════════════════════════════════════════════════════════════════
// Reset Token Generator
// ══════════════════════════════════════════════════════════════════════

#[test]
fn reset_token_unique() {
    let generator = ResetTokenGenerator::new(Duration::from_mins(5));
    let (t1, _) = generator.generate();
    let (t2, _) = generator.generate();
    assert_ne!(t1, t2);
}

#[test]
fn reset_token_is_base64url() {
    let generator = ResetTokenGenerator::new(Duration::from_mins(5));
    let (token, _) = generator.generate();
    assert!(
        token
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-'),
        "token should be base64url: {token}"
    );
}

#[test]
fn reset_token_length_is_43() {
    let generator = ResetTokenGenerator::new(Duration::from_mins(5));
    let (token, _) = generator.generate();
    assert_eq!(token.len(), 43);
}

#[test]
fn reset_token_expiry_in_future() {
    let generator = ResetTokenGenerator::new(Duration::from_mins(10));
    let (_, exp) = generator.generate();
    assert!(exp > chrono::Utc::now());
}

#[test]
fn reset_token_short_ttl() {
    let generator = ResetTokenGenerator::new(Duration::from_secs(1));
    let (_, exp) = generator.generate();
    let now = chrono::Utc::now();
    let diff = exp.signed_duration_since(now);
    assert!(diff.num_seconds() <= 2);
}

#[test]
fn reset_token_long_ttl() {
    let generator = ResetTokenGenerator::new(Duration::from_hours(24));
    let (_, exp) = generator.generate();
    let now = chrono::Utc::now();
    let diff = exp.signed_duration_since(now);
    assert!(diff.num_seconds() >= 86390);
}
