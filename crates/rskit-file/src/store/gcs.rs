//! Google Cloud Storage backend (feature: `gcs`).

#![cfg(feature = "gcs")]

use std::collections::HashMap;
use std::time::Duration;

use rskit_errors::{AppError, AppResult, ErrorCode};
use serde::Deserialize;

use crate::FileSource;

use super::{FileStore, ProgressCallback, StoredFile};

/// Configuration for the GCS store.
#[derive(Debug, Clone, Deserialize)]
pub struct GcsStoreConfig {
    /// GCS bucket name.
    pub bucket: String,
    /// Key prefix for all objects.
    pub prefix: Option<String>,
}

/// Google Cloud Storage backend.
pub struct GcsStore {
    client: google_cloud_storage::client::Client,
    config: GcsStoreConfig,
}

impl GcsStore {
    /// Create a new GCS store with the given configuration.
    pub async fn new(config: GcsStoreConfig) -> AppResult<Self> {
        let client = google_cloud_storage::client::Client::default();
        Ok(Self { client, config })
    }

    fn full_key(&self, key: &str) -> String {
        match &self.config.prefix {
            Some(prefix) => format!("{prefix}/{key}"),
            None => key.to_string(),
        }
    }
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

        use google_cloud_storage::http::objects::upload::{Media, UploadObjectRequest, UploadType};

        let upload_type = UploadType::Simple(Media::new(full_key.clone()));
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
            content_type: content_type.unwrap_or("application/octet-stream").to_string(),
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

        use google_cloud_storage::http::objects::get::GetObjectRequest;

        let req = GetObjectRequest {
            bucket: self.config.bucket.clone(),
            object: full_key,
            ..Default::default()
        };

        let data = self
            .client
            .download_object(&req, &Default::default())
            .await
            .map_err(|e| AppError::new(ErrorCode::NotFound, format!("GCS download failed: {e}")))?;

        Ok(FileSource::Bytes(bytes::Bytes::from(data)))
    }

    async fn delete(&self, key: &str) -> AppResult<()> {
        let full_key = self.full_key(key);

        use google_cloud_storage::http::objects::delete::DeleteObjectRequest;

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

        use google_cloud_storage::http::objects::get::GetObjectRequest;

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
            size: obj.size as u64,
            content_type: obj
                .content_type
                .unwrap_or_else(|| "application/octet-stream".to_string()),
            stored_at: chrono::Utc::now(),
            metadata: obj.metadata.unwrap_or_default(),
        })
    }

    async fn list(&self, prefix: &str, limit: Option<usize>) -> AppResult<Vec<StoredFile>> {
        let full_prefix = self.full_key(prefix);

        use google_cloud_storage::http::objects::list::ListObjectsRequest;

        let req = ListObjectsRequest {
            bucket: self.config.bucket.clone(),
            prefix: Some(full_prefix),
            max_results: limit.map(|l| l as i32),
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
            .map(|obj| StoredFile {
                key: obj.name,
                size: obj.size as u64,
                content_type: obj
                    .content_type
                    .unwrap_or_else(|| "application/octet-stream".to_string()),
                stored_at: chrono::Utc::now(),
                metadata: obj.metadata.unwrap_or_default(),
            })
            .collect();

        Ok(items)
    }

    async fn presigned_url(&self, _key: &str, _expires_in: Duration) -> AppResult<String> {
        Err(AppError::new(
            ErrorCode::Internal,
            "GCS presigned URLs not yet implemented",
        ))
    }

    async fn copy(&self, from_key: &str, to_key: &str) -> AppResult<StoredFile> {
        // Download and re-upload
        let source = self.download(from_key).await?;
        self.upload(&source, to_key, None, None).await
    }

    async fn rename(&self, from_key: &str, to_key: &str) -> AppResult<StoredFile> {
        let result = self.copy(from_key, to_key).await?;
        self.delete(from_key).await?;
        Ok(result)
    }
}
