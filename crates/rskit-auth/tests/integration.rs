use std::time::Duration;

use rskit_auth::{JwtConfig, JwtService, PasswordHasher, ResetTokenGenerator, TokenGenerator, TokenValidator};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
struct Claims {
    sub: String,
    exp: u64,
}

fn future_exp() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs()
        + 3600
}

fn jwt_service() -> JwtService<Claims> {
    JwtService::new(JwtConfig {
        secret: "integration-test-secret-key".into(),
        ..Default::default()
    })
}

// ── JWT ──────────────────────────────────────────────────────────────

#[tokio::test]
async fn jwt_sign_then_validate_roundtrip() {
    let svc = jwt_service();
    let claims = Claims { sub: "user-1".into(), exp: future_exp() };

    let token = svc.generate(&claims).await.unwrap();
    let decoded = svc.validate(&token).await.unwrap();

    assert_eq!(decoded, claims);
}

#[tokio::test]
async fn jwt_expired_token_fails_validation() {
    let svc = jwt_service();
    let claims = Claims { sub: "user-1".into(), exp: 1 };

    let token = svc.generate(&claims).await.unwrap();
    let result = svc.validate(&token).await;

    assert!(result.is_err());
}

// ── Password hashing ────────────────────────────────────────────────

#[test]
fn password_hash_and_verify() {
    let hasher = PasswordHasher::default();
    let hash = hasher.hash("correct-horse-battery-staple").unwrap();

    assert!(hasher.verify("correct-horse-battery-staple", &hash).unwrap());
    assert!(!hasher.verify("wrong-password", &hash).unwrap());
}

// ── Reset tokens ────────────────────────────────────────────────────

#[test]
fn reset_token_generator_returns_token_string() {
    let generator = ResetTokenGenerator::new(Duration::from_secs(600));
    let (token, _expires_at) = generator.generate();

    assert!(!token.is_empty());
}

#[test]
fn reset_token_has_expected_length() {
    // 32 random bytes → base64-URL-no-pad → 43 characters
    let generator = ResetTokenGenerator::new(Duration::from_secs(600));
    let (token, _) = generator.generate();

    assert_eq!(token.len(), 43);
}
