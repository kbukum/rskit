//! Google Cloud Storage backend implementing [`rskit_storage::store::FileStore`].

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use google_cloud_storage::http::objects::delete::DeleteObjectRequest;
use google_cloud_storage::http::objects::download::Range;
use google_cloud_storage::http::objects::get::GetObjectRequest;
use google_cloud_storage::http::objects::list::ListObjectsRequest;
use google_cloud_storage::http::objects::upload::{Media, UploadObjectRequest, UploadType};
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
    client: google_cloud_storage::client::Client,
    config: GcsStoreConfig,
}

impl GcsStore {
    /// Create a new Google Cloud Storage backend.
    pub async fn new(config: GcsStoreConfig) -> AppResult<Self> {
        let client_config = if config.anonymous {
            google_cloud_storage::client::ClientConfig::default().anonymous()
        } else {
            google_cloud_storage::client::ClientConfig::default()
                .with_auth()
                .await
                .map_err(|e| {
                    AppError::new(
                        ErrorCode::Internal,
                        format!("GCS authentication configuration failed: {e}"),
                    )
                })?
        };
        let client = google_cloud_storage::client::Client::new(client_config);
        Ok(Self { client, config })
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
}

fn object_size(size: i64) -> AppResult<u64> {
    u64::try_from(size).map_err(|_| {
        AppError::new(
            ErrorCode::Internal,
            format!("GCS object size must not be negative: {size}"),
        )
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

        let upload_type = UploadType::Simple(Media::new(full_key));
        let req = UploadObjectRequest {
            bucket: self.config.bucket.clone(),
            ..Default::default()
        };

        self.client
            .upload_object(&req, data.to_vec(), &upload_type)
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

        let req = GetObjectRequest {
            bucket: self.config.bucket.clone(),
            object: full_key,
            ..Default::default()
        };

        let data = self
            .client
            .download_object(&req, &Range::default())
            .await
            .map_err(|e| AppError::new(ErrorCode::NotFound, format!("GCS download failed: {e}")))?;

        Ok(FileSource::Bytes(bytes::Bytes::from(data)))
    }

    async fn delete(&self, key: &str) -> AppResult<()> {
        let full_key = self.full_key(key);

        let req = DeleteObjectRequest {
            bucket: self.config.bucket.clone(),
            object: full_key,
            ..Default::default()
        };

        self.client
            .delete_object(&req)
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

        let req = GetObjectRequest {
            bucket: self.config.bucket.clone(),
            object: full_key,
            ..Default::default()
        };

        let obj = self
            .client
            .get_object(&req)
            .await
            .map_err(|e| AppError::new(ErrorCode::NotFound, format!("GCS head failed: {e}")))?;

        Ok(StoredFile {
            key: key.to_string(),
            size: object_size(obj.size)?,
            content_type: obj
                .content_type
                .unwrap_or_else(|| "application/octet-stream".to_string()),
            stored_at: chrono::Utc::now(),
            metadata: obj.metadata.unwrap_or_default(),
        })
    }

    async fn list(&self, prefix: &str, limit: Option<usize>) -> AppResult<Vec<StoredFile>> {
        let full_prefix = self.full_key(prefix);

        let max_results = limit.map(i32::try_from).transpose().map_err(|_| {
            AppError::new(ErrorCode::InvalidInput, "GCS list limit exceeds i32::MAX")
        })?;

        let req = ListObjectsRequest {
            bucket: self.config.bucket.clone(),
            prefix: Some(full_prefix),
            max_results,
            ..Default::default()
        };

        let resp = self
            .client
            .list_objects(&req)
            .await
            .map_err(|e| AppError::new(ErrorCode::Internal, format!("GCS list failed: {e}")))?;

        let items = resp
            .items
            .unwrap_or_default()
            .into_iter()
            .map(|obj| {
                Ok(StoredFile {
                    key: obj.name,
                    size: object_size(obj.size)?,
                    content_type: obj
                        .content_type
                        .unwrap_or_else(|| "application/octet-stream".to_string()),
                    stored_at: chrono::Utc::now(),
                    metadata: obj.metadata.unwrap_or_default(),
                })
            })
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

struct GcsFactory;

#[async_trait::async_trait]
impl StorageFactory for GcsFactory {
    async fn create(&self, config: &StorageConfig) -> AppResult<Arc<dyn FileStore>> {
        let gcs_config: GcsStoreConfig =
            serde_json::from_value(config.options.clone()).map_err(|e| {
                AppError::new(
                    ErrorCode::InvalidInput,
                    format!("invalid GCS storage config: {e}"),
                )
            })?;
        Ok(Arc::new(GcsStore::new(gcs_config).await?))
    }
}

/// Explicitly register the Google Cloud Storage backend.
pub fn register_gcs(registry: &mut StorageRegistry) -> AppResult<()> {
    registry.register("gcs", Arc::new(GcsFactory))
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
        register_gcs(&mut registry).unwrap();
        assert!(registry.contains("gcs"));
    }
}
