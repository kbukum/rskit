//! API key rotation with grace periods.

use chrono::{DateTime, Duration, Utc};
use rskit_errors::{AppError, ErrorCode};

use super::{GenerateResult, Key, KeySpec, Manager, Store, validate};

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

impl<S: Store> Manager<S> {
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

        let old_key = self.store().get_by_id(old_key_id).await?;
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
            .issue_key(KeySpec {
                key_id: config.new_key_id.clone(),
                owner_id,
                name,
                prefix,
                scopes,
                expires_at: config.expires_at,
            })
            .await?;

        let grace_ends_at = Utc::now() + config.grace_period;
        self.store()
            .set_rotation(old_key_id, grace_ends_at, Some(record.id.clone()))
            .await?;

        Ok(RotationResult {
            issued,
            record,
            grace_ends_at,
        })
    }
}

fn invalid_input_error(message: impl Into<String>) -> AppError {
    AppError::new(ErrorCode::InvalidInput, message.into())
}

#[cfg(test)]
mod tests {
    use chrono::Utc;

    use super::{KeySpec, RotationConfig};
    use crate::apikey::Store;
    use crate::apikey::test_support::manager;

    #[tokio::test]
    async fn issue_validate_and_rotate_key() {
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
            .issue_key(KeySpec {
                key_id: String::from("key-1"),
                owner_id: String::from("owner-1"),
                name: String::from("primary"),
                prefix: String::from("pk"),
                scopes: vec![String::from("read"), String::from("write")],
                expires_at: None,
            })
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
            .issue_key(KeySpec {
                key_id: String::from("key-1"),
                owner_id: String::from("owner-1"),
                name: String::from("primary"),
                prefix: String::from("pk"),
                ..KeySpec::default()
            })
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
