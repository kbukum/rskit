//! Google Cloud Storage backend implementing [`rskit_storage::store::FileStore`].

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use google_cloud_auth::credentials::anonymous::Builder as AnonymousCredentials;
use google_cloud_storage::client::{Storage, StorageControl};
use google_cloud_storage::model::Object;
use rskit_errors::{AppError, AppResult, ErrorCode};
use rskit_storage::FileSource;
use rskit_storage::store::{
    FileStore, ProgressCallback, StorageConfig, StorageFactory, StorageRegistry, StoredFile,
};
use serde::{Deserialize, Serialize};

/// Configuration for the Google Cloud Storage backend.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GcsStoreConfig {
    /// GCS bucket name.
    pub bucket: String,
    /// Key prefix for all objects.
    pub prefix: Option<String>,
    /// Use unsigned requests for public buckets.
    ///
    /// Defaults to authenticated requests using Google application default
    /// credentials. Set this only for explicitly public buckets.
    #[serde(default)]
    pub anonymous: bool,
}

/// Google Cloud Storage backend.
pub struct GcsStore {
    storage: Storage,
    control: StorageControl,
    config: GcsStoreConfig,
}

impl GcsStore {
    /// Create a new Google Cloud Storage backend.
    pub async fn new(config: GcsStoreConfig) -> AppResult<Self> {
        let (storage, control) = if config.anonymous {
            let storage = Storage::builder()
                .with_credentials(AnonymousCredentials::new().build())
                .build()
                .await
                .map_err(|e| {
                    AppError::new(
                        ErrorCode::Internal,
                        format!("GCS anonymous storage client configuration failed: {e}"),
                    )
                })?;
            let control = StorageControl::builder()
                .with_credentials(AnonymousCredentials::new().build())
                .build()
                .await
                .map_err(|e| {
                    AppError::new(
                        ErrorCode::Internal,
                        format!("GCS anonymous control client configuration failed: {e}"),
                    )
                })?;
            (storage, control)
        } else {
            let storage = Storage::builder().build().await.map_err(|e| {
                AppError::new(
                    ErrorCode::Internal,
                    format!("GCS storage client configuration failed: {e}"),
                )
            })?;
            let control = StorageControl::builder().build().await.map_err(|e| {
                AppError::new(
                    ErrorCode::Internal,
                    format!("GCS control client configuration failed: {e}"),
                )
            })?;
            (storage, control)
        };
        Ok(Self {
            storage,
            control,
            config,
        })
    }

    /// Returns the bucket name.
    pub fn bucket(&self) -> &str {
        &self.config.bucket
    }

    fn full_key(&self, key: &str) -> String {
        self.config
            .prefix
            .as_ref()
            .map_or_else(|| key.to_string(), |prefix| format!("{prefix}/{key}"))
    }

    fn bucket_resource(&self) -> String {
        if self.config.bucket.starts_with("projects/") {
            self.config.bucket.clone()
        } else {
            format!("projects/_/buckets/{}", self.config.bucket)
        }
    }
}

fn object_size(size: i64) -> AppResult<u64> {
    u64::try_from(size).map_err(|_| {
        AppError::new(
            ErrorCode::Internal,
            format!("GCS object size must not be negative: {size}"),
        )
    })
}

fn stored_file_from_object(key: String, obj: Object) -> AppResult<StoredFile> {
    Ok(StoredFile {
        key,
        size: object_size(obj.size)?,
        content_type: if obj.content_type.is_empty() {
            "application/octet-stream".to_string()
        } else {
            obj.content_type
        },
        stored_at: chrono::Utc::now(),
        metadata: obj.metadata,
    })
}

#[async_trait::async_trait]
impl FileStore for GcsStore {
    async fn upload(
        &self,
        source: &FileSource,
        key: &str,
        content_type: Option<&str>,
        metadata: Option<HashMap<String, String>>,
    ) -> AppResult<StoredFile> {
        let data = source.read_all().await?;
        let size = data.len() as u64;
        let full_key = self.full_key(key);
        let bucket = self.bucket_resource();

        let mut request = self
            .storage
            .write_object(bucket, full_key, data)
            .set_content_type(content_type.unwrap_or("application/octet-stream"));
        if let Some(metadata) = metadata.clone() {
            request = request.set_metadata(metadata);
        }
        Box::pin(request.send_buffered())
            .await
            .map_err(|e| AppError::new(ErrorCode::Internal, format!("GCS upload failed: {e}")))?;

        Ok(StoredFile {
            key: key.to_string(),
            size,
            content_type: content_type
                .unwrap_or("application/octet-stream")
                .to_string(),
            stored_at: chrono::Utc::now(),
            metadata: metadata.unwrap_or_default(),
        })
    }

    async fn upload_with_progress(
        &self,
        source: &FileSource,
        key: &str,
        content_type: Option<&str>,
        _on_progress: ProgressCallback,
    ) -> AppResult<StoredFile> {
        self.upload(source, key, content_type, None).await
    }

    async fn download(&self, key: &str) -> AppResult<FileSource> {
        let full_key = self.full_key(key);
        let bucket = self.bucket_resource();
        let mut response = self
            .storage
            .read_object(bucket, full_key)
            .send()
            .await
            .map_err(|e| AppError::new(ErrorCode::NotFound, format!("GCS download failed: {e}")))?;

        let mut data = Vec::new();
        while let Some(chunk) = response.next().await {
            let chunk = chunk.map_err(|e| {
                AppError::new(
                    ErrorCode::Internal,
                    format!("GCS download stream failed: {e}"),
                )
            })?;
            data.extend_from_slice(&chunk);
        }

        Ok(FileSource::Bytes(bytes::Bytes::from(data)))
    }

    async fn delete(&self, key: &str) -> AppResult<()> {
        let full_key = self.full_key(key);
        let bucket = self.bucket_resource();
        self.control
            .delete_object()
            .set_bucket(bucket)
            .set_object(full_key)
            .send()
            .await
            .map_err(|e| AppError::new(ErrorCode::Internal, format!("GCS delete failed: {e}")))?;

        Ok(())
    }

    async fn exists(&self, key: &str) -> AppResult<bool> {
        match self.head(key).await {
            Ok(_) => Ok(true),
            Err(_) => Ok(false),
        }
    }

    async fn head(&self, key: &str) -> AppResult<StoredFile> {
        let full_key = self.full_key(key);
        let obj = self
            .control
            .get_object()
            .set_bucket(self.bucket_resource())
            .set_object(full_key)
            .send()
            .await
            .map_err(|e| AppError::new(ErrorCode::NotFound, format!("GCS head failed: {e}")))?;

        stored_file_from_object(key.to_string(), obj)
    }

    async fn list(&self, prefix: &str, limit: Option<usize>) -> AppResult<Vec<StoredFile>> {
        let full_prefix = self.full_key(prefix);

        let max_results = limit.map(i32::try_from).transpose().map_err(|_| {
            AppError::new(ErrorCode::InvalidInput, "GCS list limit exceeds i32::MAX")
        })?;

        let mut request = self
            .control
            .list_objects()
            .set_parent(self.bucket_resource())
            .set_prefix(full_prefix);
        if let Some(max_results) = max_results {
            request = request.set_page_size(max_results);
        }
        let resp = request
            .send()
            .await
            .map_err(|e| AppError::new(ErrorCode::Internal, format!("GCS list failed: {e}")))?;

        let items = resp
            .objects
            .into_iter()
            .map(|obj| stored_file_from_object(obj.name.clone(), obj))
            .collect::<AppResult<Vec<_>>>()?;

        Ok(items)
    }

    async fn presigned_url(&self, _key: &str, _expires_in: Duration) -> AppResult<String> {
        Err(AppError::new(
            ErrorCode::InvalidInput,
            "GCS presigned URLs are not supported by this backend",
        ))
    }

    async fn copy(&self, from_key: &str, to_key: &str) -> AppResult<StoredFile> {
        let source = self.download(from_key).await?;
        self.upload(&source, to_key, None, None).await
    }

    async fn rename(&self, from_key: &str, to_key: &str) -> AppResult<StoredFile> {
        let result = self.copy(from_key, to_key).await?;
        self.delete(from_key).await?;
        Ok(result)
    }
}

struct GcsFactory {
    config: GcsStoreConfig,
}

#[async_trait::async_trait]
impl StorageFactory for GcsFactory {
    async fn create(&self, _config: &StorageConfig) -> AppResult<Arc<dyn FileStore>> {
        Ok(Arc::new(GcsStore::new(self.config.clone()).await?))
    }
}

/// Explicitly register the Google Cloud Storage backend.
pub fn register_gcs(registry: &mut StorageRegistry, config: GcsStoreConfig) -> AppResult<()> {
    registry.register("gcs", Arc::new(GcsFactory { config }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_deserializes_with_authenticated_default() {
        let json = r#"{"bucket": "test"}"#;
        let cfg: GcsStoreConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.bucket, "test");
        assert!(cfg.prefix.is_none());
        assert!(!cfg.anonymous);
    }

    #[test]
    fn config_deserializes_anonymous_opt_in() {
        let json = r#"{"bucket": "public-assets", "prefix": "uploads", "anonymous": true}"#;
        let cfg: GcsStoreConfig = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.bucket, "public-assets");
        assert_eq!(cfg.prefix.as_deref(), Some("uploads"));
        assert!(cfg.anonymous);
    }

    #[tokio::test]
    #[ignore = "requires GCS network access; run with --include-ignored in a configured environment"]
    async fn anonymous_store_constructs_without_credentials() {
        let store = GcsStore::new(GcsStoreConfig {
            bucket: "public-assets".into(),
            prefix: Some("uploads".into()),
            anonymous: true,
        })
        .await
        .unwrap();

        assert_eq!(store.bucket(), "public-assets");
        assert_eq!(store.full_key("image.png"), "uploads/image.png");
    }

    #[test]
    fn register_gcs_adds_backend_without_constructing_client() {
        let mut registry = StorageRegistry::new();
        register_gcs(
            &mut registry,
            GcsStoreConfig {
                bucket: "test".into(),
                prefix: None,
                anonymous: true,
            },
        )
        .unwrap();
        assert!(registry.contains("gcs"));
    }
}
