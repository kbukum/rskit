//! S3 / MinIO storage backend implementing [`rskit_storage::FileStore`].

use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;
use std::time::Duration;

use rskit_errors::{AppError, AppResult, ErrorCode};
use rskit_storage::FileSource;
use rskit_storage::store::{
    FileStore, ProgressCallback, StorageConfig, StorageFactory, StorageRegistry, StoredFile,
    content_type_or_default, prefixed_key,
};
use rskit_util::env;
use serde::{Deserialize, Serialize};

/// Configuration for the S3 store.
///
/// Supports AWS S3 and S3-compatible services (MinIO, LocalStack, etc.)
/// via `endpoint`, `force_path_style`, and explicit credentials.
#[derive(Clone, Deserialize, Serialize)]
pub struct Config {
    /// S3 bucket name.
    pub bucket: String,
    /// AWS region (e.g., `"us-east-1"`). Falls back to `AWS_REGION` or
    /// `AWS_DEFAULT_REGION` env vars if unset.
    pub region: Option<String>,
    /// Custom endpoint URL for S3-compatible services (e.g.,
    /// `"http://localhost:9000"` for MinIO).
    pub endpoint: Option<String>,
    /// Key prefix applied to all objects (e.g., `"uploads"`).
    pub prefix: Option<String>,
    /// Use path-style access (`http://host/bucket/key`) instead of
    /// virtual-hosted-style (`http://bucket.host/key`).
    /// Required for MinIO and most S3-compatible services.
    #[serde(default)]
    pub force_path_style: bool,
    /// Explicit AWS access key ID. Falls back to `AWS_ACCESS_KEY_ID` env var.
    pub access_key_id: Option<String>,
    /// Explicit AWS secret access key. Falls back to `AWS_SECRET_ACCESS_KEY` env var.
    pub secret_access_key: Option<String>,
}

impl fmt::Debug for Config {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Config")
            .field("bucket", &self.bucket)
            .field("region", &self.region)
            .field("endpoint", &self.endpoint)
            .field("prefix", &self.prefix)
            .field("force_path_style", &self.force_path_style)
            .field(
                "access_key_id",
                &self.access_key_id.as_ref().map(|_| "<redacted>"),
            )
            .field(
                "secret_access_key",
                &self.secret_access_key.as_ref().map(|_| "<redacted>"),
            )
            .finish()
    }
}

/// Amazon S3 / MinIO storage backend.
///
/// Created by registering [`Config`] with the storage registry.
/// Implements [`FileStore`] for use with any rskit-storage consumer.
struct S3Store {
    client: aws_sdk_s3::Client,
    config: Config,
}

impl S3Store {
    /// Create a new S3 store from config.
    ///
    /// Resolves credentials from config fields, then env vars.
    /// The client is synchronously constructed and reused for all operations.
    fn new(config: Config) -> AppResult<Self> {
        let client = aws_sdk_s3::Client::from_conf(client_config_builder(&config)?.build());
        Ok(Self { client, config })
    }

    fn full_key(&self, key: &str) -> String {
        prefixed_key(self.config.prefix.as_deref(), key)
    }
}

/// Build the AWS SDK configuration for a store from validated [`Config`].
///
/// Kept separate from [`S3Store::new`] so tests can attach a wire-level mock
/// HTTP client to the same builder the production path uses.
fn client_config_builder(config: &Config) -> AppResult<aws_sdk_s3::config::Builder> {
    let (access_key, secret_key) = resolve_credentials(config)?;

    let creds = aws_sdk_s3::config::Credentials::new(
        &access_key,
        &secret_key,
        None,
        None,
        "rskit-storage-s3",
    );

    let mut builder = aws_sdk_s3::Config::builder()
        .credentials_provider(creds)
        .behavior_version_latest();

    // Region: config → AWS_REGION → AWS_DEFAULT_REGION
    if let Some(region) = &config.region {
        builder = builder.region(aws_sdk_s3::config::Region::new(region.clone()));
    } else if let Some(region) = env::get_non_empty("AWS_REGION") {
        builder = builder.region(aws_sdk_s3::config::Region::new(region));
    } else if let Some(region) = env::get_non_empty("AWS_DEFAULT_REGION") {
        builder = builder.region(aws_sdk_s3::config::Region::new(region));
    }

    if let Some(endpoint) = &config.endpoint {
        builder = builder.endpoint_url(endpoint);
    }

    if config.force_path_style {
        builder = builder.force_path_style(true);
    }

    Ok(builder)
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

        req = req.content_type(content_type_or_default(content_type));
        if let Some(meta) = &metadata {
            for (k, v) in meta {
                req = req.metadata(k, v);
            }
        }

        req.send()
            .await
            .map_err(|e| AppError::new(ErrorCode::Internal, format!("S3 upload failed: {e}")))?;

        Ok(uploaded_file(key, size, content_type, metadata))
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

        let data =
            resp.body.collect().await.map_err(|e| {
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

        Ok(stored_file_from_head(key, &resp))
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

        let resp = req
            .send()
            .await
            .map_err(|e| AppError::new(ErrorCode::Internal, format!("S3 list failed: {e}")))?;

        let items = resp
            .contents()
            .iter()
            .map(stored_file_from_object)
            .collect();

        Ok(items)
    }

    async fn presigned_url(&self, key: &str, expires_in: Duration) -> AppResult<String> {
        let full_key = self.full_key(key);
        let presigning_config = aws_sdk_s3::presigning::PresigningConfig::expires_in(expires_in)
            .map_err(|e| {
                AppError::new(
                    ErrorCode::InvalidInput,
                    format!("Invalid presigning duration: {e}"),
                )
            })?;

        let presigned = self
            .client
            .get_object()
            .bucket(&self.config.bucket)
            .key(&full_key)
            .presigned(presigning_config)
            .await
            .map_err(|e| {
                AppError::new(
                    ErrorCode::Internal,
                    format!("S3 presigned URL generation failed: {e}"),
                )
            })?;

        Ok(presigned.uri().to_string())
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

fn uploaded_file(
    key: &str,
    size: u64,
    content_type: Option<&str>,
    metadata: Option<HashMap<String, String>>,
) -> StoredFile {
    StoredFile::new(prefixed_key(None, key), size, content_type)
        .with_metadata(metadata.unwrap_or_default())
}

fn stored_file_from_head(
    key: &str,
    resp: &aws_sdk_s3::operation::head_object::HeadObjectOutput,
) -> StoredFile {
    StoredFile::new(
        prefixed_key(None, key),
        resp.content_length().unwrap_or(0) as u64,
        resp.content_type(),
    )
    .with_metadata(
        resp.metadata()
            .map(|m| m.iter().map(|(k, v)| (k.clone(), v.clone())).collect())
            .unwrap_or_default(),
    )
}

fn stored_file_from_object(obj: &aws_sdk_s3::types::Object) -> StoredFile {
    StoredFile::new(
        obj.key().unwrap_or(""),
        obj.size().unwrap_or(0) as u64,
        None,
    )
}

/// Resolve AWS credentials from config fields or environment variables.
fn resolve_credentials(config: &Config) -> AppResult<(String, String)> {
    resolve_credentials_with(config, env::get_non_empty)
}

fn resolve_credentials_with(
    config: &Config,
    get_env: impl Fn(&str) -> Option<String>,
) -> AppResult<(String, String)> {
    if let (Some(key), Some(secret)) = (&config.access_key_id, &config.secret_access_key)
        && !key.is_empty()
        && !secret.is_empty()
    {
        return Ok((key.clone(), secret.clone()));
    }

    let key = get_env("AWS_ACCESS_KEY_ID");
    let secret = get_env("AWS_SECRET_ACCESS_KEY");

    let (Some(key), Some(secret)) = (key, secret) else {
        return Err(AppError::new(
            ErrorCode::MissingField,
            "S3 credentials not found. Set access_key_id/secret_access_key in config \
             or AWS_ACCESS_KEY_ID/AWS_SECRET_ACCESS_KEY environment variables.",
        ));
    };

    Ok((key, secret))
}

struct S3Factory {
    config: Config,
}

#[async_trait::async_trait]
impl StorageFactory for S3Factory {
    async fn create(&self, _config: &StorageConfig) -> AppResult<Arc<dyn FileStore>> {
        Ok(Arc::new(S3Store::new(self.config.clone())?))
    }
}

/// Explicitly register the S3 backend in an injected storage registry.
pub fn register(registry: &mut StorageRegistry, config: Config) -> AppResult<()> {
    registry.register("s3", Arc::new(S3Factory { config }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_deserializes_with_defaults() {
        let json = r#"{"bucket": "test", "endpoint": "http://localhost:9000"}"#;
        let cfg: Config = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.bucket, "test");
        assert!(!cfg.force_path_style);
        assert!(cfg.access_key_id.is_none());
        assert!(cfg.region.is_none());
    }

    #[test]
    fn config_deserializes_full() {
        let json = r#"{
            "bucket": "assets",
            "region": "us-east-1",
            "endpoint": "http://minio:9000",
            "prefix": "uploads",
            "force_path_style": true,
            "access_key_id": "minio",
            "secret_access_key": "minio123"
        }"#;
        let cfg: Config = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.bucket, "assets");
        assert!(cfg.force_path_style);
        assert_eq!(cfg.access_key_id.as_deref(), Some("minio"));
    }

    #[test]
    fn config_debug_redacts_credentials() {
        let cfg = Config {
            bucket: "assets".into(),
            region: Some("us-east-1".into()),
            endpoint: Some("https://s3.example.test".into()),
            prefix: Some("uploads".into()),
            force_path_style: true,
            access_key_id: Some("access-key".into()),
            secret_access_key: Some("secret-key".into()),
        };

        let debug = format!("{cfg:?}");

        assert!(debug.contains("<redacted>"));
        assert!(!debug.contains("access-key"));
        assert!(!debug.contains("secret-key"));
        assert!(debug.contains("assets"));
    }

    #[test]
    fn config_debug_omits_redaction_marker_without_credentials() {
        let cfg = Config {
            bucket: "assets".into(),
            region: None,
            endpoint: None,
            prefix: None,
            force_path_style: false,
            access_key_id: None,
            secret_access_key: None,
        };

        let debug = format!("{cfg:?}");

        assert!(debug.contains("access_key_id: None"));
        assert!(debug.contains("secret_access_key: None"));
        assert!(!debug.contains("<redacted>"));
    }

    #[test]
    fn resolve_explicit_credentials() {
        let cfg = Config {
            bucket: "test".into(),
            region: None,
            endpoint: None,
            prefix: None,
            force_path_style: false,
            access_key_id: Some("key123".into()),
            secret_access_key: Some("secret456".into()),
        };
        let (key, secret) = resolve_credentials(&cfg).unwrap();
        assert_eq!(key, "key123");
        assert_eq!(secret, "secret456");
    }

    #[test]
    fn resolve_blank_explicit_credentials_falls_back_to_error_without_env() {
        let cfg = Config {
            bucket: "test".into(),
            region: None,
            endpoint: None,
            prefix: None,
            force_path_style: false,
            access_key_id: Some(String::new()),
            secret_access_key: Some(String::new()),
        };

        let err = resolve_credentials_with(&cfg, |_| None).unwrap_err();

        assert_eq!(err.code(), ErrorCode::MissingField);
    }

    #[test]
    fn resolve_empty_credentials_errors() {
        let cfg = Config {
            bucket: "test".into(),
            region: None,
            endpoint: None,
            prefix: None,
            force_path_style: false,
            access_key_id: None,
            secret_access_key: None,
        };
        let err = resolve_credentials_with(&cfg, |_| None).unwrap_err();

        assert_eq!(err.code(), ErrorCode::MissingField);
    }

    #[test]
    fn resolve_credentials_uses_environment_when_explicit_credentials_absent() {
        let cfg = Config {
            bucket: "test".into(),
            region: None,
            endpoint: None,
            prefix: None,
            force_path_style: false,
            access_key_id: None,
            secret_access_key: None,
        };

        let (key, secret) = resolve_credentials_with(&cfg, |name| match name {
            "AWS_ACCESS_KEY_ID" => Some("env-key".to_string()),
            "AWS_SECRET_ACCESS_KEY" => Some("env-secret".to_string()),
            _ => None,
        })
        .unwrap();

        assert_eq!(key, "env-key");
        assert_eq!(secret, "env-secret");
    }

    #[test]
    fn full_key_with_prefix() {
        let store = S3Store::new(Config {
            bucket: "b".into(),
            region: None,
            endpoint: None,
            prefix: Some("pfx".into()),
            force_path_style: false,
            access_key_id: Some("k".into()),
            secret_access_key: Some("s".into()),
        })
        .unwrap();
        assert_eq!(store.full_key("file.txt"), "pfx/file.txt");
    }

    #[test]
    fn full_key_with_slash_only_prefix() {
        let store = S3Store::new(Config {
            bucket: "b".into(),
            region: None,
            endpoint: None,
            prefix: Some("///".into()),
            force_path_style: false,
            access_key_id: Some("k".into()),
            secret_access_key: Some("s".into()),
        })
        .unwrap();

        assert_eq!(store.full_key("file.txt"), "file.txt");
    }

    #[test]
    fn full_key_without_prefix() {
        let store = S3Store::new(Config {
            bucket: "b".into(),
            region: None,
            endpoint: None,
            prefix: None,
            force_path_style: false,
            access_key_id: Some("k".into()),
            secret_access_key: Some("s".into()),
        })
        .unwrap();
        assert_eq!(store.full_key("file.txt"), "file.txt");
    }

    #[test]
    fn register_rejects_duplicate_s3_backend() {
        let mut registry = StorageRegistry::new();
        let config = Config {
            bucket: "b".into(),
            region: None,
            endpoint: None,
            prefix: None,
            force_path_style: false,
            access_key_id: Some("k".into()),
            secret_access_key: Some("s".into()),
        };

        register(&mut registry, config.clone()).unwrap();
        let err = register(&mut registry, config).unwrap_err();

        assert_eq!(err.code(), ErrorCode::AlreadyExists);
    }

    #[test]
    fn store_construction_applies_region_endpoint_and_path_style() {
        let store = S3Store::new(Config {
            bucket: "b".into(),
            region: Some("us-east-1".into()),
            endpoint: Some("http://127.0.0.1:9000".into()),
            prefix: Some("uploads".into()),
            force_path_style: true,
            access_key_id: Some("k".into()),
            secret_access_key: Some("s".into()),
        })
        .unwrap();

        assert_eq!(store.full_key("file.txt"), "uploads/file.txt");
    }

    #[tokio::test]
    async fn factory_creates_store_from_explicit_credentials() {
        let factory = S3Factory {
            config: Config {
                bucket: "b".into(),
                region: Some("us-east-1".into()),
                endpoint: Some("http://127.0.0.1:9000".into()),
                prefix: None,
                force_path_style: true,
                access_key_id: Some("k".into()),
                secret_access_key: Some("s".into()),
            },
        };

        factory.create(&StorageConfig::default()).await.unwrap();
    }

    #[tokio::test]
    async fn presigned_url_validates_duration_and_uses_configured_key_prefix() {
        let store = S3Store::new(Config {
            bucket: "bucket".into(),
            region: Some("us-east-1".into()),
            endpoint: Some("http://127.0.0.1:9000".into()),
            prefix: Some("uploads".into()),
            force_path_style: true,
            access_key_id: Some("access".into()),
            secret_access_key: Some("secret".into()),
        })
        .unwrap();

        let too_long = store
            .presigned_url("file.txt", Duration::from_secs(60 * 60 * 24 * 8))
            .await
            .unwrap_err();
        assert_eq!(too_long.code(), ErrorCode::InvalidInput);

        let url = store
            .presigned_url("file.txt", Duration::from_secs(60))
            .await
            .unwrap();
        assert!(url.contains("uploads/file.txt"));
    }

    #[test]
    fn stored_file_mappers_preserve_keys_sizes_content_types_and_metadata() {
        let mut metadata = HashMap::new();
        metadata.insert("owner".to_string(), "media".to_string());
        let uploaded = uploaded_file("file.txt", 4, Some("text/plain"), Some(metadata.clone()));
        assert_eq!(uploaded.key, "file.txt");
        assert_eq!(uploaded.size, 4);
        assert_eq!(uploaded.content_type, "text/plain");
        assert_eq!(uploaded.metadata, metadata);

        let head = aws_sdk_s3::operation::head_object::HeadObjectOutput::builder()
            .content_length(9)
            .content_type("application/json")
            .metadata("trace", "abc")
            .build();
        let from_head = stored_file_from_head("meta.json", &head);
        assert_eq!(from_head.key, "meta.json");
        assert_eq!(from_head.size, 9);
        assert_eq!(from_head.content_type, "application/json");
        assert_eq!(
            from_head.metadata.get("trace").map(String::as_str),
            Some("abc")
        );

        let object = aws_sdk_s3::types::Object::builder()
            .key("uploads/a.txt")
            .size(12)
            .build();
        let from_object = stored_file_from_object(&object);
        assert_eq!(from_object.key, "uploads/a.txt");
        assert_eq!(from_object.size, 12);

        let empty_object = aws_sdk_s3::types::Object::builder().build();
        let from_empty_object = stored_file_from_object(&empty_object);
        assert_eq!(from_empty_object.key, "");
        assert_eq!(from_empty_object.size, 0);
    }

    // --- Wire-level operation tests -----------------------------------------
    //
    // These exercise request construction and response/error mapping for every
    // `FileStore` operation against an in-process mock HTTP client. No network,
    // credentials, or live S3 service are involved.

    use aws_smithy_http_client::test_util::{ReplayEvent, StaticReplayClient};
    use aws_smithy_types::body::SdkBody;

    fn wire_config(prefix: Option<&str>) -> Config {
        Config {
            bucket: "test-bucket".into(),
            region: Some("us-east-1".into()),
            endpoint: Some("http://s3.local".into()),
            prefix: prefix.map(str::to_owned),
            force_path_style: true,
            access_key_id: Some("test-key".into()),
            secret_access_key: Some("test-secret".into()),
        }
    }

    /// Build a store whose AWS client replays the given responses in order.
    fn wire_store(config: Config, events: Vec<ReplayEvent>) -> (S3Store, StaticReplayClient) {
        let http = StaticReplayClient::new(events);
        let conf = client_config_builder(&config)
            .unwrap()
            .http_client(http.clone())
            .build();
        (
            S3Store {
                client: aws_sdk_s3::Client::from_conf(conf),
                config,
            },
            http,
        )
    }

    fn ok_response(status: u16, body: impl Into<SdkBody>) -> ReplayEvent {
        ReplayEvent::new(
            http::Request::builder().body(SdkBody::empty()).unwrap(),
            http::Response::builder()
                .status(status)
                .body(body.into())
                .unwrap(),
        )
    }

    fn error_response(status: u16, code: &str) -> ReplayEvent {
        let body = format!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\
             <Error><Code>{code}</Code><Message>mock failure</Message>\
             <RequestId>req-1</RequestId></Error>"
        );
        ok_response(status, SdkBody::from(body))
    }

    #[tokio::test]
    async fn upload_puts_prefixed_object_and_returns_logical_key() {
        let (store, http) = wire_store(wire_config(Some("uploads")), vec![ok_response(200, "")]);
        let mut metadata = HashMap::new();
        metadata.insert("owner".to_string(), "media".to_string());

        let stored = store
            .upload(
                &FileSource::Bytes(bytes::Bytes::from_static(b"payload")),
                "file.txt",
                Some("text/plain"),
                Some(metadata),
            )
            .await
            .unwrap();

        assert_eq!(stored.key, "file.txt");
        assert_eq!(stored.size, 7);
        assert_eq!(stored.content_type, "text/plain");
        assert_eq!(
            stored.metadata.get("owner").map(String::as_str),
            Some("media")
        );

        let request = http.actual_requests().next().expect("a request was sent");
        assert_eq!(request.method(), "PUT");
        assert!(
            request.uri().contains("/test-bucket/uploads/file.txt"),
            "unexpected upload uri: {}",
            request.uri()
        );
    }

    #[tokio::test]
    async fn upload_maps_remote_failure_to_internal_error() {
        let (store, _http) = wire_store(
            wire_config(None),
            vec![error_response(500, "InternalError")],
        );

        let err = store
            .upload(
                &FileSource::Bytes(bytes::Bytes::from_static(b"x")),
                "file.txt",
                None,
                None,
            )
            .await
            .unwrap_err();

        assert_eq!(err.code(), ErrorCode::Internal);
        assert!(err.message().contains("S3 upload failed"));
    }

    #[tokio::test]
    async fn download_collects_object_body_into_bytes() {
        let (store, http) = wire_store(wire_config(None), vec![ok_response(200, "downloaded")]);

        let source = store.download("file.txt").await.unwrap();
        let data = source.read_all().await.unwrap();
        assert_eq!(data.as_ref(), b"downloaded");

        let request = http.actual_requests().next().unwrap();
        assert_eq!(request.method(), "GET");
        assert!(request.uri().contains("/test-bucket/file.txt"));
    }

    #[tokio::test]
    async fn download_maps_missing_object_to_not_found() {
        let (store, _http) = wire_store(wire_config(None), vec![error_response(404, "NoSuchKey")]);

        let err = store.download("missing.txt").await.unwrap_err();

        assert_eq!(err.code(), ErrorCode::NotFound);
        assert!(err.message().contains("S3 download failed"));
    }

    #[tokio::test]
    async fn delete_sends_delete_object_request() {
        let (store, http) = wire_store(wire_config(None), vec![ok_response(204, "")]);

        store.delete("file.txt").await.unwrap();

        let request = http.actual_requests().next().unwrap();
        assert_eq!(request.method(), "DELETE");
        assert!(request.uri().contains("/test-bucket/file.txt"));
    }

    #[tokio::test]
    async fn delete_maps_remote_failure_to_internal_error() {
        let (store, _http) =
            wire_store(wire_config(None), vec![error_response(403, "AccessDenied")]);

        let err = store.delete("file.txt").await.unwrap_err();

        assert_eq!(err.code(), ErrorCode::Internal);
        assert!(err.message().contains("S3 delete failed"));
    }

    #[tokio::test]
    async fn exists_reports_presence_from_head_outcome() {
        let (present, _) = wire_store(wire_config(None), vec![ok_response(200, "")]);
        assert!(present.exists("file.txt").await.unwrap());

        let (absent, _) = wire_store(wire_config(None), vec![error_response(404, "NotFound")]);
        assert!(!absent.exists("file.txt").await.unwrap());
    }

    #[tokio::test]
    async fn head_maps_headers_to_stored_file_metadata() {
        let response = ReplayEvent::new(
            http::Request::builder().body(SdkBody::empty()).unwrap(),
            http::Response::builder()
                .status(200)
                .header("content-length", "9")
                .header("content-type", "application/json")
                .header("x-amz-meta-trace", "abc")
                .body(SdkBody::empty())
                .unwrap(),
        );
        let (store, _http) = wire_store(wire_config(Some("uploads")), vec![response]);

        let stored = store.head("meta.json").await.unwrap();

        assert_eq!(stored.key, "meta.json");
        assert_eq!(stored.size, 9);
        assert_eq!(stored.content_type, "application/json");
        assert_eq!(
            stored.metadata.get("trace").map(String::as_str),
            Some("abc")
        );
    }

    #[tokio::test]
    async fn head_maps_missing_object_to_not_found() {
        let (store, _http) = wire_store(wire_config(None), vec![error_response(404, "NoSuchKey")]);

        let err = store.head("missing.txt").await.unwrap_err();

        assert_eq!(err.code(), ErrorCode::NotFound);
        assert!(err.message().contains("S3 head failed"));
    }

    #[tokio::test]
    async fn list_parses_contents_and_sends_prefix() {
        let body = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\
            <ListBucketResult xmlns=\"http://s3.amazonaws.com/doc/2006-03-01/\">\
            <Name>test-bucket</Name><Prefix>uploads/logs</Prefix>\
            <KeyCount>2</KeyCount><MaxKeys>2</MaxKeys><IsTruncated>false</IsTruncated>\
            <Contents><Key>uploads/logs/a.txt</Key><Size>12</Size>\
            <LastModified>2024-01-01T00:00:00.000Z</LastModified></Contents>\
            <Contents><Key>uploads/logs/b.txt</Key><Size>34</Size>\
            <LastModified>2024-01-01T00:00:00.000Z</LastModified></Contents>\
            </ListBucketResult>";
        let (store, http) = wire_store(
            wire_config(Some("uploads")),
            vec![ok_response(200, SdkBody::from(body))],
        );

        let items = store.list("logs", Some(2)).await.unwrap();

        assert_eq!(items.len(), 2);
        assert_eq!(items[0].key, "uploads/logs/a.txt");
        assert_eq!(items[0].size, 12);
        assert_eq!(items[1].size, 34);

        let request = http.actual_requests().next().unwrap();
        assert!(request.uri().contains("prefix=uploads%2Flogs"));
        assert!(request.uri().contains("max-keys=2"));
    }

    #[tokio::test]
    async fn list_maps_remote_failure_to_internal_error() {
        let (store, _http) = wire_store(
            wire_config(None),
            vec![error_response(500, "InternalError")],
        );

        let err = store.list("logs", None).await.unwrap_err();

        assert_eq!(err.code(), ErrorCode::Internal);
        assert!(err.message().contains("S3 list failed"));
    }

    #[tokio::test]
    async fn copy_copies_source_then_heads_destination() {
        let copy_body = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\
            <CopyObjectResult xmlns=\"http://s3.amazonaws.com/doc/2006-03-01/\">\
            <ETag>\"abc\"</ETag><LastModified>2024-01-01T00:00:00.000Z</LastModified>\
            </CopyObjectResult>";
        let head = ReplayEvent::new(
            http::Request::builder().body(SdkBody::empty()).unwrap(),
            http::Response::builder()
                .status(200)
                .header("content-length", "5")
                .header("content-type", "text/plain")
                .body(SdkBody::empty())
                .unwrap(),
        );
        let (store, http) = wire_store(
            wire_config(Some("uploads")),
            vec![ok_response(200, SdkBody::from(copy_body)), head],
        );

        let stored = store.copy("a.txt", "b.txt").await.unwrap();

        assert_eq!(stored.key, "b.txt");
        assert_eq!(stored.size, 5);

        let mut requests = http.actual_requests();
        let copy = requests.next().unwrap();
        assert_eq!(copy.method(), "PUT");
        assert!(copy.uri().contains("/test-bucket/uploads/b.txt"));
        assert_eq!(
            copy.headers().get("x-amz-copy-source"),
            Some("test-bucket/uploads/a.txt")
        );
        assert_eq!(requests.next().unwrap().method(), "HEAD");
    }

    #[tokio::test]
    async fn copy_maps_remote_failure_to_internal_error() {
        let (store, _http) = wire_store(wire_config(None), vec![error_response(404, "NoSuchKey")]);

        let err = store.copy("a.txt", "b.txt").await.unwrap_err();

        assert_eq!(err.code(), ErrorCode::Internal);
        assert!(err.message().contains("S3 copy failed"));
    }

    #[tokio::test]
    async fn rename_copies_then_deletes_source() {
        let copy_body = "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\
            <CopyObjectResult xmlns=\"http://s3.amazonaws.com/doc/2006-03-01/\">\
            <ETag>\"abc\"</ETag><LastModified>2024-01-01T00:00:00.000Z</LastModified>\
            </CopyObjectResult>";
        let head = ReplayEvent::new(
            http::Request::builder().body(SdkBody::empty()).unwrap(),
            http::Response::builder()
                .status(200)
                .header("content-length", "5")
                .body(SdkBody::empty())
                .unwrap(),
        );
        let (store, http) = wire_store(
            wire_config(None),
            vec![
                ok_response(200, SdkBody::from(copy_body)),
                head,
                ok_response(204, ""),
            ],
        );

        let stored = store.rename("a.txt", "b.txt").await.unwrap();

        assert_eq!(stored.key, "b.txt");

        let methods: Vec<String> = http
            .actual_requests()
            .map(|r| r.method().to_owned())
            .collect();
        assert_eq!(
            methods.iter().map(String::as_str).collect::<Vec<_>>(),
            ["PUT", "HEAD", "DELETE"]
        );
    }
}
