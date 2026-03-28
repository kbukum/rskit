//! Amazon S3 storage backend (feature: `s3`).

#![cfg(feature = "s3")]

use std::collections::HashMap;
use std::time::Duration;

use rskit_errors::{AppError, AppResult, ErrorCode};
use serde::Deserialize;

use crate::FileSource;

use super::{FileStore, ProgressCallback, StoredFile};

/// Configuration for the S3 store.
#[derive(Debug, Clone, Deserialize)]
pub struct S3StoreConfig {
    /// S3 bucket name.
    pub bucket: String,
    /// AWS region (e.g., "us-east-1").
    pub region: Option<String>,
    /// Custom endpoint URL (for S3-compatible services).
    pub endpoint: Option<String>,
    /// Key prefix for all objects.
    pub prefix: Option<String>,
}

/// Amazon S3 storage backend.
pub struct S3Store {
    client: aws_sdk_s3::Client,
    config: S3StoreConfig,
}

impl S3Store {
    /// Create a new S3 store with the given configuration.
    pub async fn new(config: S3StoreConfig) -> AppResult<Self> {
        let mut aws_config = aws_config::defaults(aws_config::BehaviorVersion::latest());

        if let Some(region) = &config.region {
            aws_config = aws_config.region(aws_sdk_s3::config::Region::new(region.clone()));
        }

        let sdk_config = aws_config.load().await;
        let mut s3_config = aws_sdk_s3::config::Builder::from(&sdk_config);

        if let Some(endpoint) = &config.endpoint {
            s3_config = s3_config.endpoint_url(endpoint);
        }

        let client = aws_sdk_s3::Client::from_conf(s3_config.build());

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
impl FileStore for S3Store {
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

        let mut req = self
            .client
            .put_object()
            .bucket(&self.config.bucket)
            .key(&full_key)
            .body(data.to_vec().into());

        if let Some(ct) = content_type {
            req = req.content_type(ct);
        }
        if let Some(meta) = &metadata {
            for (k, v) in meta {
                req = req.metadata(k, v);
            }
        }

        req.send().await.map_err(|e| {
            AppError::new(ErrorCode::Internal, format!("S3 upload failed: {e}"))
        })?;

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
        let resp = self
            .client
            .get_object()
            .bucket(&self.config.bucket)
            .key(&full_key)
            .send()
            .await
            .map_err(|e| AppError::new(ErrorCode::NotFound, format!("S3 download failed: {e}")))?;

        let data = resp.body.collect().await.map_err(|e| {
            AppError::new(ErrorCode::Internal, format!("S3 read body failed: {e}"))
        })?;

        Ok(FileSource::Bytes(bytes::Bytes::from(data.to_vec())))
    }

    async fn delete(&self, key: &str) -> AppResult<()> {
        let full_key = self.full_key(key);
        self.client
            .delete_object()
            .bucket(&self.config.bucket)
            .key(&full_key)
            .send()
            .await
            .map_err(|e| AppError::new(ErrorCode::Internal, format!("S3 delete failed: {e}")))?;
        Ok(())
    }

    async fn exists(&self, key: &str) -> AppResult<bool> {
        let full_key = self.full_key(key);
        match self
            .client
            .head_object()
            .bucket(&self.config.bucket)
            .key(&full_key)
            .send()
            .await
        {
            Ok(_) => Ok(true),
            Err(_) => Ok(false),
        }
    }

    async fn head(&self, key: &str) -> AppResult<StoredFile> {
        let full_key = self.full_key(key);
        let resp = self
            .client
            .head_object()
            .bucket(&self.config.bucket)
            .key(&full_key)
            .send()
            .await
            .map_err(|e| AppError::new(ErrorCode::NotFound, format!("S3 head failed: {e}")))?;

        Ok(StoredFile {
            key: key.to_string(),
            size: resp.content_length().unwrap_or(0) as u64,
            content_type: resp
                .content_type()
                .unwrap_or("application/octet-stream")
                .to_string(),
            stored_at: chrono::Utc::now(),
            metadata: resp
                .metadata()
                .map(|m| m.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
                .unwrap_or_default(),
        })
    }

    async fn list(&self, prefix: &str, limit: Option<usize>) -> AppResult<Vec<StoredFile>> {
        let full_prefix = self.full_key(prefix);
        let mut req = self
            .client
            .list_objects_v2()
            .bucket(&self.config.bucket)
            .prefix(&full_prefix);

        if let Some(max) = limit {
            req = req.max_keys(max as i32);
        }

        let resp = req.send().await.map_err(|e| {
            AppError::new(ErrorCode::Internal, format!("S3 list failed: {e}"))
        })?;

        let items = resp.contents().iter().map(|obj| {
            StoredFile {
                key: obj.key().unwrap_or("").to_string(),
                size: obj.size().unwrap_or(0) as u64,
                content_type: "application/octet-stream".to_string(),
                stored_at: chrono::Utc::now(),
                metadata: HashMap::new(),
            }
        }).collect();

        Ok(items)
    }

    async fn presigned_url(&self, _key: &str, _expires_in: Duration) -> AppResult<String> {
        Err(AppError::new(
            ErrorCode::Internal,
            "S3 presigned URLs require the presigning module",
        ))
    }

    async fn copy(&self, from_key: &str, to_key: &str) -> AppResult<StoredFile> {
        let full_from = self.full_key(from_key);
        let full_to = self.full_key(to_key);

        self.client
            .copy_object()
            .bucket(&self.config.bucket)
            .copy_source(format!("{}/{}", self.config.bucket, full_from))
            .key(&full_to)
            .send()
            .await
            .map_err(|e| AppError::new(ErrorCode::Internal, format!("S3 copy failed: {e}")))?;

        self.head(to_key).await
    }

    async fn rename(&self, from_key: &str, to_key: &str) -> AppResult<StoredFile> {
        let result = self.copy(from_key, to_key).await?;
        self.delete(from_key).await?;
        Ok(result)
    }
}
