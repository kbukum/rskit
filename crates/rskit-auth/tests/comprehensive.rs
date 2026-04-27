use std::time::Duration;

use rskit_auth::{
    JwtConfig, JwtService, PasswordHasher, ResetTokenGenerator, TokenGenerator, TokenValidator,
};
use serde::{Deserialize, Serialize};

// ── Test Claims ─────────────────────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct Claims {
    sub: String,
    exp: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct ClaimsWithIssAud {
    sub: String,
    exp: u64,
    iss: String,
    aud: Vec<String>,
}

fn future_exp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + 3600
}

fn past_exp() -> u64 {
    1 // Unix epoch + 1 second, definitely expired
}

fn jwt_service() -> JwtService<Claims> {
    JwtService::new(JwtConfig::new("test-secret-key-for-auth"))
}

// ══════════════════════════════════════════════════════════════════════
// JWT — Generate & Validate
// ══════════════════════════════════════════════════════════════════════

#[tokio::test]
async fn jwt_roundtrip_preserves_claims() {
    let svc = jwt_service();
    let claims = Claims {
        sub: "user-abc".into(),
        exp: future_exp(),
    };
    let token = svc.generate(&claims).await.unwrap();
    let decoded = svc.validate(&token).await.unwrap();
    assert_eq!(decoded.sub, "user-abc");
}

#[tokio::test]
async fn jwt_expired_token_rejected() {
    let svc = jwt_service();
    let claims = Claims {
        sub: "user-1".into(),
        exp: past_exp(),
    };
    let token = svc.generate(&claims).await.unwrap();
    assert!(svc.validate(&token).await.is_err());
}

#[tokio::test]
async fn jwt_invalid_signature_rejected() {
    let svc1 = JwtService::<Claims>::new(JwtConfig::new("secret-one"));
    let svc2 = JwtService::<Claims>::new(JwtConfig::new("secret-two"));
    let claims = Claims {
        sub: "u".into(),
        exp: future_exp(),
    };
    let token = svc1.generate(&claims).await.unwrap();
    assert!(svc2.validate(&token).await.is_err());
}

#[tokio::test]
async fn jwt_empty_token_rejected() {
    let svc = jwt_service();
    assert!(svc.validate("").await.is_err());
}

#[tokio::test]
async fn jwt_garbage_token_rejected() {
    let svc = jwt_service();
    assert!(svc.validate("not.a.jwt").await.is_err());
}

#[tokio::test]
async fn jwt_no_dots_rejected() {
    let svc = jwt_service();
    assert!(svc.validate("nodots").await.is_err());
}

#[tokio::test]
async fn jwt_issuer_validation() {
    let svc_gen = JwtService::<ClaimsWithIssAud>::new(
        JwtConfig::new("shared")
            .with_issuer("issuer-a")
            .with_audience(vec!["aud".into()]),
    );
    let svc_val = JwtService::<ClaimsWithIssAud>::new(
        JwtConfig::new("shared")
            .with_issuer("issuer-b")
            .with_audience(vec!["aud".into()]),
    );
    let claims = ClaimsWithIssAud {
        sub: "u".into(),
        exp: future_exp(),
        iss: "issuer-a".into(),
        aud: vec!["aud".into()],
    };
    let token = svc_gen.generate(&claims).await.unwrap();
    assert!(svc_val.validate(&token).await.is_err());
}

#[tokio::test]
async fn jwt_audience_validation() {
    let svc_gen = JwtService::<ClaimsWithIssAud>::new(
        JwtConfig::new("shared").with_audience(vec!["aud-a".into()]),
    );
    let svc_val = JwtService::<ClaimsWithIssAud>::new(
        JwtConfig::new("shared").with_audience(vec!["aud-b".into()]),
    );
    let claims = ClaimsWithIssAud {
        sub: "u".into(),
        exp: future_exp(),
        iss: String::new(),
        aud: vec!["aud-a".into()],
    };
    let token = svc_gen.generate(&claims).await.unwrap();
    assert!(svc_val.validate(&token).await.is_err());
}

#[tokio::test]
async fn jwt_large_claims_roundtrip() {
    let svc = jwt_service();
    let claims = Claims {
        sub: "x".repeat(10_000),
        exp: future_exp(),
    };
    let token = svc.generate(&claims).await.unwrap();
    let decoded = svc.validate(&token).await.unwrap();
    assert_eq!(decoded.sub.len(), 10_000);
}

#[tokio::test]
async fn jwt_special_chars_in_claims() {
    let svc = jwt_service();
    let claims = Claims {
        sub: "用户/special<chars>&\"quotes\"".into(),
        exp: future_exp(),
    };
    let token = svc.generate(&claims).await.unwrap();
    let decoded = svc.validate(&token).await.unwrap();
    assert_eq!(decoded.sub, claims.sub);
}

#[tokio::test]
async fn jwt_concurrent_validation() {
    let svc = jwt_service();
    let claims = Claims {
        sub: "user-concurrent".into(),
        exp: future_exp(),
    };
    let token = svc.generate(&claims).await.unwrap();

    let mut handles = vec![];
    for _ in 0..20 {
        let svc_clone = jwt_service();
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
                let pw = format!("password-{}", i);
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
    let generator = ResetTokenGenerator::new(Duration::from_secs(300));
    let (t1, _) = generator.generate();
    let (t2, _) = generator.generate();
    assert_ne!(t1, t2);
}

#[test]
fn reset_token_is_base64url() {
    let generator = ResetTokenGenerator::new(Duration::from_secs(300));
    let (token, _) = generator.generate();
    // base64url-no-pad: only [A-Za-z0-9_-]
    assert!(
        token
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-'),
        "token should be base64url: {token}"
    );
}

#[test]
fn reset_token_length_is_43() {
    // 32 random bytes → base64-URL-no-pad → 43 characters
    let generator = ResetTokenGenerator::new(Duration::from_secs(300));
    let (token, _) = generator.generate();
    assert_eq!(token.len(), 43);
}

#[test]
fn reset_token_expiry_in_future() {
    let generator = ResetTokenGenerator::new(Duration::from_secs(600));
    let (_, exp) = generator.generate();
    assert!(exp > chrono::Utc::now());
}

#[test]
fn reset_token_short_ttl() {
    let generator = ResetTokenGenerator::new(Duration::from_secs(1));
    let (_, exp) = generator.generate();
    let now = chrono::Utc::now();
    // Should expire within 2 seconds of now
    let diff = exp.signed_duration_since(now);
    assert!(diff.num_seconds() <= 2);
}

#[test]
fn reset_token_long_ttl() {
    let generator = ResetTokenGenerator::new(Duration::from_secs(86400)); // 24 hours
    let (_, exp) = generator.generate();
    let now = chrono::Utc::now();
    let diff = exp.signed_duration_since(now);
    assert!(diff.num_seconds() >= 86390); // allow small timing tolerance
}
