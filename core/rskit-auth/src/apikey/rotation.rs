//! API key manager and rotation support.

use chrono::{DateTime, Duration, Utc};
use rskit_errors::{AppError, ErrorCode};

use super::{GenerateResult, Hasher, Key, Store, split_key, validate};

/// Default grace period: 7 days.
pub const DEFAULT_GRACE_PERIOD: Duration = Duration::days(7);

/// Rotation settings for API keys.
#[derive(Debug, Clone)]
pub struct RotationConfig {
    /// Grace period for the old key.
    pub grace_period: Duration,
    /// Replacement key identifier.
    pub new_key_id: String,
    /// Replacement key owner.
    pub owner_id: String,
    /// Replacement key display name.
    pub name: String,
    /// Replacement key prefix.
    pub prefix: String,
    /// Replacement scopes. Empty reuses the existing key scopes.
    pub scopes: Vec<String>,
    /// Optional replacement expiry.
    pub expires_at: Option<DateTime<Utc>>,
}

impl Default for RotationConfig {
    fn default() -> Self {
        Self {
            grace_period: DEFAULT_GRACE_PERIOD,
            new_key_id: String::new(),
            owner_id: String::new(),
            name: String::new(),
            prefix: String::new(),
            scopes: Vec::new(),
            expires_at: None,
        }
    }
}

/// Rotation result.
#[derive(Debug, Clone)]
pub struct RotationResult {
    /// Newly issued key material.
    pub issued: GenerateResult,
    /// Persisted replacement record.
    pub record: Key,
    /// Grace period end for the old key.
    pub grace_ends_at: DateTime<Utc>,
}

/// Manager for issuing, validating, and rotating API keys.
pub struct Manager<S> {
    store: S,
    hasher: Hasher,
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
    pub async fn issue_key(
        &self,
        key_id: &str,
        owner_id: &str,
        name: &str,
        prefix: &str,
        scopes: &[String],
        expires_at: Option<DateTime<Utc>>,
    ) -> Result<(GenerateResult, Key), AppError> {
        let issued = self.hasher.generate_key(prefix)?;
        let record = Key {
            id: key_id.to_string(),
            owner_id: owner_id.to_string(),
            name: name.to_string(),
            key_prefix: issued.key_prefix.clone(),
            key_digest: issued.key_digest.clone(),
            scopes: scopes.to_vec(),
            is_active: true,
            expires_at,
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

    /// Rotate a key and issue a replacement.
    pub async fn rotate_key(
        &self,
        old_key_id: &str,
        config: RotationConfig,
    ) -> Result<RotationResult, AppError> {
        if config.new_key_id.is_empty() {
            return Err(AppError::invalid_input(
                "new_key_id",
                "new_key_id is required for rotation",
            ));
        }

        let old_key = self.store.get_by_id(old_key_id).await?;
        validate(&old_key)
            .map_err(|error| invalid_input_error(format!("cannot rotate key: {error}")))?;

        let scopes = if config.scopes.is_empty() {
            old_key.scopes.clone()
        } else {
            config.scopes.clone()
        };
        let owner_id = if config.owner_id.is_empty() {
            old_key.owner_id.clone()
        } else {
            config.owner_id.clone()
        };
        let name = if config.name.is_empty() {
            old_key.name.clone()
        } else {
            config.name.clone()
        };
        let prefix = if config.prefix.is_empty() {
            old_key.key_prefix.clone()
        } else {
            config.prefix.clone()
        };

        let (issued, record) = self
            .issue_key(
                &config.new_key_id,
                &owner_id,
                &name,
                &prefix,
                &scopes,
                config.expires_at,
            )
            .await?;

        let grace_ends_at = Utc::now() + config.grace_period;
        self.store
            .set_rotation(old_key_id, grace_ends_at, Some(record.id.clone()))
            .await?;

        Ok(RotationResult {
            issued,
            record,
            grace_ends_at,
        })
    }
}

#[async_trait::async_trait]
impl<S: Store> super::KeyValidator for Manager<S> {
    async fn validate_key(&self, plain_key: &str) -> Result<Key, AppError> {
        self.validate_key_with_scopes(plain_key, &[]).await
    }
}

fn invalid_input_error(message: impl Into<String>) -> AppError {
    AppError::new(ErrorCode::InvalidInput, message.into())
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use async_trait::async_trait;
    use chrono::{DateTime, Utc};
    use parking_lot::Mutex;
    use rskit_errors::AppError;

    use super::{Manager, RotationConfig};
    use crate::apikey::{Hasher, HashingConfig, Key, Store};

    #[derive(Default)]
    struct MemoryStore {
        keys: Mutex<HashMap<String, Key>>,
    }

    #[async_trait]
    impl Store for MemoryStore {
        async fn create(&self, key: Key) -> Result<(), AppError> {
            self.keys.lock().insert(key.id.clone(), key);
            Ok(())
        }

        async fn list_by_prefix(&self, key_prefix: &str) -> Result<Vec<Key>, AppError> {
            Ok(self
                .keys
                .lock()
                .values()
                .filter(|key| key.key_prefix == key_prefix)
                .cloned()
                .collect())
        }

        async fn get_by_id(&self, key_id: &str) -> Result<Key, AppError> {
            self.keys
                .lock()
                .get(key_id)
                .cloned()
                .ok_or_else(|| AppError::not_found("API key", Some(key_id)))
        }

        async fn update_last_used(
            &self,
            key_id: &str,
            used_at: DateTime<Utc>,
        ) -> Result<(), AppError> {
            if let Some(key) = self.keys.lock().get_mut(key_id) {
                key.last_used_at = Some(used_at);
            }
            Ok(())
        }

        async fn set_rotation(
            &self,
            key_id: &str,
            grace_ends_at: DateTime<Utc>,
            rotated_by_id: Option<String>,
        ) -> Result<(), AppError> {
            if let Some(key) = self.keys.lock().get_mut(key_id) {
                key.grace_ends_at = Some(grace_ends_at);
                key.rotated_by_id = rotated_by_id;
            }
            Ok(())
        }

        async fn set_active(&self, key_id: &str, active: bool) -> Result<(), AppError> {
            if let Some(key) = self.keys.lock().get_mut(key_id) {
                key.is_active = active;
            }
            Ok(())
        }

        async fn delete(&self, key_id: &str) -> Result<(), AppError> {
            self.keys.lock().remove(key_id);
            Ok(())
        }
    }

    fn manager() -> Manager<MemoryStore> {
        Manager::new(
            MemoryStore::default(),
            Hasher::new(HashingConfig {
                pepper: "p".repeat(32),
                entropy_bytes: 32,
            })
            .unwrap(),
        )
    }

    #[tokio::test]
    async fn issue_validate_and_rotate_key() {
        let manager = manager();
        let (issued, record) = manager
            .issue_key(
                "key-1",
                "user-1",
                "primary",
                "pk",
                &[String::from("read")],
                None,
            )
            .await
            .unwrap();
        assert_eq!(record.key_prefix, "pk");

        let validated = manager
            .validate_key_with_scopes(&issued.plain_key, &[String::from("read")])
            .await
            .unwrap();
        assert_eq!(validated.owner_id, "user-1");
        assert!(validated.last_used_at.is_some());

        let rotation = manager
            .rotate_key(
                "key-1",
                RotationConfig {
                    new_key_id: String::from("key-2"),
                    owner_id: String::from("user-1"),
                    name: String::from("secondary"),
                    prefix: String::from("pk"),
                    ..RotationConfig::default()
                },
            )
            .await
            .unwrap();
        assert_eq!(rotation.record.id, "key-2");
        let original = manager.store().get_by_id("key-1").await.unwrap();
        assert_eq!(original.rotated_by_id.as_deref(), Some("key-2"));
    }

    #[tokio::test]
    async fn rotate_key_inherits_existing_metadata_when_config_fields_are_empty() {
        let manager = manager();
        let (_issued, original) = manager
            .issue_key(
                "key-1",
                "owner-1",
                "primary",
                "pk",
                &[String::from("read"), String::from("write")],
                None,
            )
            .await
            .unwrap();

        let rotation = manager
            .rotate_key(
                &original.id,
                RotationConfig {
                    new_key_id: String::from("key-2"),
                    ..RotationConfig::default()
                },
            )
            .await
            .unwrap();

        assert_eq!(rotation.record.owner_id, "owner-1");
        assert_eq!(rotation.record.name, "primary");
        assert_eq!(rotation.record.key_prefix, "pk");
        assert_eq!(
            rotation.record.scopes,
            vec![String::from("read"), String::from("write")]
        );
        assert!(rotation.grace_ends_at > Utc::now());
        assert!(!rotation.issued.plain_key.is_empty());
    }

    #[tokio::test]
    async fn rotate_key_rejects_missing_new_key_id_before_mutating_store() {
        let manager = manager();
        manager
            .issue_key("key-1", "owner-1", "primary", "pk", &[], None)
            .await
            .unwrap();

        let error = manager
            .rotate_key("key-1", RotationConfig::default())
            .await
            .unwrap_err();

        assert_eq!(error.code(), rskit_errors::ErrorCode::InvalidInput);
        let original = manager.store().get_by_id("key-1").await.unwrap();
        assert!(original.rotated_by_id.is_none());
    }
}
