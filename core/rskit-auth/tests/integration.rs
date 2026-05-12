#![allow(missing_docs)]

mod common;

use std::time::Duration;

use common::{StandardClaims, jwt_service};
use rskit_auth::{PasswordHasher, ResetTokenGenerator, TokenGenerator, TokenValidator};

// ── JWT ──────────────────────────────────────────────────────────────

#[tokio::test]
async fn jwt_sign_then_validate_roundtrip() {
    let svc = jwt_service("integration-test-secret-key");
    let claims = StandardClaims::new("user-1");

    let token = svc.generate(&claims).await.unwrap();
    let decoded = svc.validate(&token).await.unwrap();

    assert_eq!(decoded, claims);
}

#[tokio::test]
async fn jwt_expired_token_fails_validation() {
    let svc = jwt_service("integration-test-secret-key");
    let mut claims = StandardClaims::new("user-1");
    claims.exp = 1;

    let token = svc.generate(&claims).await.unwrap();
    let result = svc.validate(&token).await;

    assert!(result.is_err());
}

// ── Password hashing ────────────────────────────────────────────────

#[test]
fn password_hash_and_verify() {
    let hasher = PasswordHasher::default();
    let hash = hasher.hash("correct-horse-battery-staple").unwrap();

    assert!(
        hasher
            .verify("correct-horse-battery-staple", &hash)
            .unwrap()
    );
    assert!(!hasher.verify("wrong-password", &hash).unwrap());
}

// ── Reset tokens ────────────────────────────────────────────────────

#[test]
fn reset_token_generator_returns_token_string() {
    let generator = ResetTokenGenerator::new(Duration::from_mins(10));
    let (token, _expires_at) = generator.generate();

    assert!(!token.is_empty());
}

#[test]
fn reset_token_has_expected_length() {
    // 32 random bytes → base64-URL-no-pad → 43 characters
    let generator = ResetTokenGenerator::new(Duration::from_mins(10));
    let (token, _) = generator.generate();

    assert_eq!(token.len(), 43);
}
