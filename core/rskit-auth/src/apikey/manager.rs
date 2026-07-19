//! API key manager: issuing and validating keys.

use chrono::{DateTime, Utc};
use rskit_errors::{AppError, ErrorCode};

use super::{GenerateResult, Hasher, Key, Store, split_key, validate};

/// Specification of a new API key to issue.
#[derive(Debug, Clone, Default)]
pub struct KeySpec {
    /// Key identifier.
    pub key_id: String,
    /// Key owner.
    pub owner_id: String,
    /// Display name.
    pub name: String,
    /// Key prefix.
    pub prefix: String,
    /// Granted scopes.
    pub scopes: Vec<String>,
    /// Optional expiry.
    pub expires_at: Option<DateTime<Utc>>,
}

/// Manager for issuing, validating, and rotating API keys.
pub struct Manager<S> {
    pub(super) store: S,
    pub(super) hasher: Hasher,
}

impl<S> Manager<S> {
    /// Construct a manager.
    #[must_use]
    pub const fn new(store: S, hasher: Hasher) -> Self {
        Self { store, hasher }
    }

    /// Access the configured store.
    #[must_use]
    pub const fn store(&self) -> &S {
        &self.store
    }

    /// Access the configured hasher.
    #[must_use]
    pub const fn hasher(&self) -> &Hasher {
        &self.hasher
    }
}

impl<S: Store> Manager<S> {
    /// Issue and persist a new key.
    pub async fn issue_key(&self, spec: KeySpec) -> Result<(GenerateResult, Key), AppError> {
        let issued = self.hasher.generate_key(&spec.prefix)?;
        let record = Key {
            id: spec.key_id,
            owner_id: spec.owner_id,
            name: spec.name,
            key_prefix: issued.key_prefix.clone(),
            key_digest: issued.key_digest.clone(),
            scopes: spec.scopes,
            is_active: true,
            expires_at: spec.expires_at,
            grace_ends_at: None,
            rotated_by_id: None,
            last_used_at: None,
            created_at: Utc::now(),
        };
        self.store.create(record.clone()).await?;
        Ok((issued, record))
    }

    /// Validate a plaintext key.
    pub async fn validate_key_with_scopes(
        &self,
        plain_key: &str,
        required_scopes: &[String],
    ) -> Result<Key, AppError> {
        let (key_prefix, _secret) = split_key(plain_key)?;
        let candidates = self.store.list_by_prefix(&key_prefix).await?;

        let mut matched: Option<Key> = None;
        for candidate in candidates {
            let digest_matches = self.hasher.compare(plain_key, &candidate.key_digest);
            if digest_matches && matched.is_none() {
                matched = Some(candidate);
            }
        }

        let mut matched = matched.ok_or_else(AppError::invalid_token)?;
        validate(&matched).map_err(|_| AppError::invalid_token())?;
        if required_scopes
            .iter()
            .any(|scope| !matched.scopes.iter().any(|granted| granted == scope))
        {
            return Err(AppError::new(
                ErrorCode::Forbidden,
                String::from("insufficient API key scope"),
            ));
        }

        let used_at = Utc::now();
        self.store.update_last_used(&matched.id, used_at).await?;
        matched.last_used_at = Some(used_at);
        Ok(matched)
    }
}

#[async_trait::async_trait]
impl<S: Store> super::KeyValidator for Manager<S> {
    async fn validate_key(&self, plain_key: &str) -> Result<Key, AppError> {
        self.validate_key_with_scopes(plain_key, &[]).await
    }
}

#[cfg(test)]
mod tests {
    use super::KeySpec;
    use crate::apikey::test_support::manager;

    #[tokio::test]
    async fn issue_persists_key_and_validates_with_required_scopes() {
        let manager = manager();
        let (issued, record) = manager
            .issue_key(KeySpec {
                key_id: String::from("key-1"),
                owner_id: String::from("user-1"),
                name: String::from("primary"),
                prefix: String::from("pk"),
                scopes: vec![String::from("read")],
                expires_at: None,
            })
            .await
            .unwrap();
        assert_eq!(record.key_prefix, "pk");

        let validated = manager
            .validate_key_with_scopes(&issued.plain_key, &[String::from("read")])
            .await
            .unwrap();
        assert_eq!(validated.owner_id, "user-1");
        assert!(validated.last_used_at.is_some());
    }

    #[tokio::test]
    async fn validate_rejects_missing_required_scope() {
        let manager = manager();
        let (issued, _record) = manager
            .issue_key(KeySpec {
                key_id: String::from("key-1"),
                owner_id: String::from("user-1"),
                name: String::from("primary"),
                prefix: String::from("pk"),
                scopes: vec![String::from("read")],
                expires_at: None,
            })
            .await
            .unwrap();

        let error = manager
            .validate_key_with_scopes(&issued.plain_key, &[String::from("write")])
            .await
            .unwrap_err();
        assert_eq!(error.code(), rskit_errors::ErrorCode::Forbidden);
    }
}
