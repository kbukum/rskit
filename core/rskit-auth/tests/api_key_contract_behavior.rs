#![allow(missing_docs)]

use std::time::Duration;

use chrono::{Duration as ChronoDuration, Utc};
use rskit_auth::apikey::{Hasher, HashingConfig, Key, KeyValidationError, split_key, validate};
use rskit_auth::{AuthClaims, AuthOutcome, MissingCredentialPolicy, ResetTokenGenerator};

fn key(
    is_active: bool,
    expires_at: Option<chrono::DateTime<Utc>>,
    grace_ends_at: Option<chrono::DateTime<Utc>>,
) -> Key {
    Key {
        id: "key-1".into(),
        owner_id: "owner".into(),
        name: "test".into(),
        key_prefix: "pk".into(),
        key_digest: "digest".into(),
        scopes: vec!["read".into()],
        is_active,
        expires_at,
        grace_ends_at,
        rotated_by_id: None,
        last_used_at: None,
        created_at: Utc::now(),
    }
}

#[test]
fn api_key_hashing_validates_config_generates_split_and_compares_digest() {
    assert!(Hasher::new(HashingConfig::new("short")).is_err());
    assert!(
        Hasher::new(HashingConfig {
            pepper: "p".repeat(32),
            entropy_bytes: 8
        })
        .is_err()
    );

    let hasher = Hasher::new(HashingConfig {
        pepper: "p".repeat(32),
        entropy_bytes: 0,
    })
    .unwrap();
    assert_eq!(hasher.config().entropy_bytes, 32);
    let issued = hasher.generate_key("pk_test").unwrap();
    let (prefix, secret) = split_key(&issued.plain_key).unwrap();
    assert_eq!(prefix, "pk_test");
    assert!(!secret.is_empty());
    assert_eq!(issued.key_prefix, "pk_test");
    assert!(hasher.compare(&issued.plain_key, &issued.key_digest));
    assert!(!hasher.compare("pk_test.wrong", &issued.key_digest));
    assert!(hasher.generate_key("bad prefix").is_err());
    assert!(split_key("missing-separator").is_err());
    assert!(split_key(".secret").is_err());
}

#[test]
fn key_validation_distinguishes_revoked_expired_and_grace_period() {
    assert!(validate(&key(true, None, None)).is_ok());
    assert_eq!(
        validate(&key(false, None, None)).unwrap_err().to_string(),
        KeyValidationError::Revoked.to_string()
    );
    assert!(key(true, Some(Utc::now() - ChronoDuration::minutes(5)), None).is_expired_past_grace());
    assert!(matches!(
        validate(&key(
            true,
            Some(Utc::now() - ChronoDuration::minutes(5)),
            None
        )),
        Err(KeyValidationError::Expired)
    ));
    assert!(
        !key(
            true,
            Some(Utc::now() - ChronoDuration::minutes(5)),
            Some(Utc::now() + ChronoDuration::minutes(5))
        )
        .is_expired_past_grace()
    );
    assert!(key(true, None, Some(Utc::now() - ChronoDuration::minutes(1))).is_expired_past_grace());
}

#[test]
fn auth_outcome_claims_and_missing_policy_are_typed() {
    assert_eq!(
        MissingCredentialPolicy::default(),
        MissingCredentialPolicy::RejectMissing
    );
    let claims = AuthClaims("subject");
    assert_eq!(claims.0, "subject");
    let authenticated = AuthOutcome::Authenticated(claims.0);
    assert_eq!(authenticated.claims(), Some(&"subject"));
    assert!(!authenticated.is_missing());
    let missing = AuthOutcome::<&str>::Missing;
    assert!(missing.is_missing());
    assert_eq!(missing.claims(), None);
}

#[test]
fn reset_tokens_are_url_safe_and_expire_after_ttl() {
    let generator = ResetTokenGenerator::new(Duration::from_secs(60));
    let (token, expires_at) = generator.generate();
    assert_eq!(token.len(), 43);
    assert!(
        token
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || ch == '-' || ch == '_')
    );
    assert!(expires_at > Utc::now());
}
