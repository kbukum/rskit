//! Google Cloud Storage backend implementing [`rskit_storage::store::FileStore`].

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::Once;
use std::time::Duration;

use google_cloud_auth::credentials::anonymous::Builder as AnonymousCredentials;
use google_cloud_storage::client::{Storage, StorageControl};
use google_cloud_storage::model::Object;
use rskit_errors::{AppError, AppResult, ErrorCode};
use rskit_storage::FileSource;
use rskit_storage::store::{
    FileStore, ProgressCallback, StorageConfig, StorageFactory, StorageRegistry, StoredFile,
    content_type_or_default, prefixed_key,
};
use serde::{Deserialize, Serialize};
use tokio::sync::OnceCell;

/// Configuration for the Google Cloud Storage backend.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Config {
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
struct GcsStore<S = google_cloud_storage::stub::DefaultStorage>
where
    S: google_cloud_storage::stub::Storage + 'static,
{
    clients: OnceCell<GcsClients<S>>,
    builder: GcsClientBuilder<S>,
    config: Config,
}

struct GcsClients<S>
where
    S: google_cloud_storage::stub::Storage + 'static,
{
    storage: Storage<S>,
    control: StorageControl,
}

type GcsClientFuture<S> = Pin<Box<dyn Future<Output = AppResult<GcsClients<S>>> + Send>>;
type GcsClientBuilder<S> = Box<dyn Fn() -> GcsClientFuture<S> + Send + Sync>;

impl GcsStore {
    /// Create a new Google Cloud Storage backend.
    fn new(config: Config) -> Self {
        let builder_config = config.clone();
        let builder = Box::new(move || {
            let config = builder_config.clone();
            Box::pin(async move {
                install_rustls_crypto_provider();

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

                Ok(GcsClients { storage, control })
            }) as GcsClientFuture<google_cloud_storage::stub::DefaultStorage>
        });

        Self {
            clients: OnceCell::new(),
            builder,
            config,
        }
    }
}

fn install_rustls_crypto_provider() {
    static INSTALL: Once = Once::new();
    INSTALL.call_once(|| {
        let _ = rustls::crypto::aws_lc_rs::default_provider().install_default();
    });
}

impl<S> GcsStore<S>
where
    S: google_cloud_storage::stub::Storage + 'static,
{
    fn full_key(&self, key: &str) -> String {
        full_key(self.config.prefix.as_deref(), key)
    }

    fn bucket_resource(&self) -> String {
        bucket_resource(&self.config.bucket)
    }

    async fn clients(&self) -> AppResult<&GcsClients<S>> {
        self.clients.get_or_try_init(|| (self.builder)()).await
    }
}

fn full_key(prefix: Option<&str>, key: &str) -> String {
    prefixed_key(prefix, key)
}

fn bucket_resource(bucket: &str) -> String {
    if bucket.starts_with("projects/") {
        bucket.to_owned()
    } else {
        format!("projects/_/buckets/{bucket}")
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
    Ok(
        StoredFile::new(key, object_size(obj.size)?, Some(&obj.content_type))
            .with_metadata(obj.metadata),
    )
}

#[async_trait::async_trait]
impl<S> FileStore for GcsStore<S>
where
    S: google_cloud_storage::stub::Storage + 'static,
{
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
        let clients = self.clients().await?;

        let mut request = clients
            .storage
            .write_object(bucket, full_key, data)
            .set_content_type(content_type_or_default(content_type));
        if let Some(metadata) = metadata.clone() {
            request = request.set_metadata(metadata);
        }
        Box::pin(request.send_buffered())
            .await
            .map_err(|e| AppError::new(ErrorCode::Internal, format!("GCS upload failed: {e}")))?;

        Ok(StoredFile::new(prefixed_key(None, key), size, content_type)
            .with_metadata(metadata.unwrap_or_default()))
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
        let clients = self.clients().await?;
        let mut response = clients
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
        let clients = self.clients().await?;
        clients
            .control
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
        let clients = self.clients().await?;
        let obj = clients
            .control
            .get_object()
            .set_bucket(self.bucket_resource())
            .set_object(full_key)
            .send()
            .await
            .map_err(|e| AppError::new(ErrorCode::NotFound, format!("GCS head failed: {e}")))?;

        stored_file_from_object(prefixed_key(None, key), obj)
    }

    async fn list(&self, prefix: &str, limit: Option<usize>) -> AppResult<Vec<StoredFile>> {
        let full_prefix = self.full_key(prefix);

        let max_results = limit.map(i32::try_from).transpose().map_err(|_| {
            AppError::new(ErrorCode::InvalidInput, "GCS list limit exceeds i32::MAX")
        })?;

        let clients = self.clients().await?;
        let mut request = clients
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
    config: Config,
}

#[async_trait::async_trait]
impl StorageFactory for GcsFactory {
    async fn create(&self, _config: &StorageConfig) -> AppResult<Arc<dyn FileStore>> {
        Ok(Arc::new(GcsStore::new(self.config.clone())))
    }
}

/// Explicitly register the Google Cloud Storage backend.
pub fn register(registry: &mut StorageRegistry, config: Config) -> AppResult<()> {
    registry.register("gcs", Arc::new(GcsFactory { config }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use google_cloud_gax::options::RequestOptions as GaxRequestOptions;
    use google_cloud_gax::response::Response;
    use google_cloud_storage::model::{
        DeleteObjectRequest, GetObjectRequest, ListObjectsRequest, ListObjectsResponse,
        ReadObjectRequest,
    };
    use google_cloud_storage::model_ext::{ObjectHighlights, WriteObjectRequest};
    use google_cloud_storage::read_object::ReadObjectResponse;
    use google_cloud_storage::request_options::RequestOptions as StorageRequestOptions;
    use google_cloud_storage::streaming_source::StreamingSource;

    #[derive(Debug, Default)]
    struct MockStorage;

    impl google_cloud_storage::stub::Storage for MockStorage {
        async fn read_object(
            &self,
            req: ReadObjectRequest,
            _options: StorageRequestOptions,
        ) -> google_cloud_storage::Result<ReadObjectResponse> {
            assert_eq!(req.bucket, "projects/_/buckets/test-bucket");
            assert_eq!(req.object, "assets/input.txt");
            Ok(ReadObjectResponse::from_source(
                ObjectHighlights::default(),
                "downloaded",
            ))
        }

        async fn write_object_buffered<P>(
            &self,
            _payload: P,
            req: WriteObjectRequest,
            _options: StorageRequestOptions,
        ) -> google_cloud_storage::Result<Object>
        where
            P: StreamingSource + Send + Sync + 'static,
        {
            let resource = req.spec.resource.expect("write resource");
            assert_eq!(resource.bucket, "projects/_/buckets/test-bucket");
            assert_eq!(resource.name, "assets/output.txt");
            assert!(
                resource.content_type == "text/plain"
                    || resource.content_type == "application/octet-stream"
            );
            if resource.content_type == "text/plain" && !resource.metadata.is_empty() {
                assert_eq!(
                    resource.metadata.get("trace").map(String::as_str),
                    Some("yes")
                );
            }
            Ok(resource)
        }
    }

    #[derive(Debug, Default)]
    struct MockControl;

    impl google_cloud_storage::stub::StorageControl for MockControl {
        async fn delete_object(
            &self,
            req: DeleteObjectRequest,
            _options: GaxRequestOptions,
        ) -> google_cloud_storage::Result<Response<()>> {
            assert_eq!(req.bucket, "projects/_/buckets/test-bucket");
            assert_eq!(req.object, "assets/input.txt");
            Ok(Response::from(()))
        }

        async fn get_object(
            &self,
            req: GetObjectRequest,
            _options: GaxRequestOptions,
        ) -> google_cloud_storage::Result<Response<Object>> {
            assert_eq!(req.bucket, "projects/_/buckets/test-bucket");
            assert_eq!(req.object, "assets/input.txt");
            Ok(Response::from(object(
                "assets/input.txt",
                10,
                "text/plain",
                [("origin", "head")],
            )))
        }

        async fn list_objects(
            &self,
            req: ListObjectsRequest,
            _options: GaxRequestOptions,
        ) -> google_cloud_storage::Result<Response<ListObjectsResponse>> {
            assert_eq!(req.parent, "projects/_/buckets/test-bucket");
            assert_eq!(req.prefix, "assets/logs");
            assert_eq!(req.page_size, 2);
            let response = ListObjectsResponse::new().set_objects([
                object("assets/logs/a.txt", 1, "text/plain", []),
                object("assets/logs/b.txt", 2, "text/plain", [("kind", "log")]),
            ]);
            Ok(Response::from(response))
        }
    }

    fn object<const N: usize>(
        name: &str,
        size: i64,
        content_type: &str,
        metadata: [(&str, &str); N],
    ) -> Object {
        Object::default()
            .set_bucket("projects/_/buckets/test-bucket")
            .set_name(name)
            .set_size(size)
            .set_content_type(content_type)
            .set_metadata(
                metadata
                    .into_iter()
                    .map(|(k, v)| (k.to_owned(), v.to_owned())),
            )
    }

    fn test_store() -> GcsStore<MockStorage> {
        GcsStore {
            clients: OnceCell::new_with(Some(GcsClients {
                storage: Storage::from_stub(MockStorage),
                control: StorageControl::from_stub(MockControl),
            })),
            builder: Box::new(|| {
                Box::pin(async {
                    Err(AppError::new(
                        ErrorCode::Internal,
                        "test GCS client builder should not be called",
                    ))
                })
            }),
            config: Config {
                bucket: "test-bucket".into(),
                prefix: Some("assets".into()),
                anonymous: true,
            },
        }
    }

    #[test]
    fn config_deserializes_with_authenticated_default() {
        let json = r#"{"bucket": "test"}"#;
        let cfg: Config = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.bucket, "test");
        assert!(cfg.prefix.is_none());
        assert!(!cfg.anonymous);
    }

    #[test]
    fn config_deserializes_anonymous_opt_in() {
        let json = r#"{"bucket": "public-assets", "prefix": "uploads", "anonymous": true}"#;
        let cfg: Config = serde_json::from_str(json).unwrap();
        assert_eq!(cfg.bucket, "public-assets");
        assert_eq!(cfg.prefix.as_deref(), Some("uploads"));
        assert!(cfg.anonymous);
    }

    #[test]
    fn key_and_bucket_helpers_normalize_inputs() {
        assert_eq!(full_key(Some("/assets/"), "/image.png"), "assets/image.png");
        assert_eq!(full_key(Some("///"), "image.png"), "image.png");
        assert_eq!(full_key(None, "/image.png"), "image.png");
        assert_eq!(
            bucket_resource("plain-bucket"),
            "projects/_/buckets/plain-bucket"
        );
        assert_eq!(
            bucket_resource("projects/p/buckets/named"),
            "projects/p/buckets/named"
        );
    }

    #[test]
    fn object_size_rejects_negative_values() {
        let err = object_size(-1).unwrap_err();
        assert_eq!(err.code(), ErrorCode::Internal);
        assert!(err.message().contains("must not be negative"));
    }

    #[test]
    fn stored_file_from_object_preserves_metadata() {
        let stored = stored_file_from_object(
            "logical.txt".into(),
            object("assets/logical.txt", 42, "text/plain", [("owner", "test")]),
        )
        .unwrap();
        assert_eq!(stored.key, "logical.txt");
        assert_eq!(stored.size, 42);
        assert_eq!(stored.content_type, "text/plain");
        assert_eq!(
            stored.metadata.get("owner").map(String::as_str),
            Some("test")
        );
    }

    #[tokio::test]
    async fn upload_writes_prefixed_object_and_returns_logical_key() {
        let store = test_store();
        let mut metadata = HashMap::new();
        metadata.insert("trace".into(), "yes".into());
        let stored = store
            .upload(
                &FileSource::Bytes(bytes::Bytes::from_static(b"payload")),
                "output.txt",
                Some("text/plain"),
                Some(metadata),
            )
            .await
            .unwrap();

        assert_eq!(stored.key, "output.txt");
        assert_eq!(stored.size, 7);
        assert_eq!(stored.content_type, "text/plain");
        assert_eq!(
            stored.metadata.get("trace").map(String::as_str),
            Some("yes")
        );
    }

    #[tokio::test]
    async fn upload_defaults_content_type_and_empty_metadata() {
        let store = test_store();
        let stored = store
            .upload(
                &FileSource::Bytes(bytes::Bytes::from_static(b"payload")),
                "output.txt",
                None,
                None,
            )
            .await
            .unwrap();

        assert_eq!(stored.key, "output.txt");
        assert_eq!(stored.size, 7);
        assert_eq!(stored.content_type, "application/octet-stream");
        assert!(stored.metadata.is_empty());
    }

    #[tokio::test]
    async fn upload_with_progress_delegates_to_upload() {
        let store = test_store();
        let stored = store
            .upload_with_progress(
                &FileSource::Bytes(bytes::Bytes::from_static(b"payload")),
                "output.txt",
                Some("text/plain"),
                Arc::new(|_| {}),
            )
            .await
            .unwrap();

        assert_eq!(stored.key, "output.txt");
        assert_eq!(stored.size, 7);
    }

    #[tokio::test]
    async fn download_reads_all_chunks_into_bytes() {
        let store = test_store();
        let source = store.download("input.txt").await.unwrap();
        let data = source.read_all().await.unwrap();
        assert_eq!(data.as_ref(), b"downloaded");
    }

    #[tokio::test]
    async fn delete_head_exists_and_list_use_control_client() {
        let store = test_store();
        store.delete("input.txt").await.unwrap();

        let head = store.head("input.txt").await.unwrap();
        assert_eq!(head.key, "input.txt");
        assert_eq!(head.size, 10);
        assert_eq!(
            head.metadata.get("origin").map(String::as_str),
            Some("head")
        );
        assert!(store.exists("input.txt").await.unwrap());

        let listed = store.list("logs", Some(2)).await.unwrap();
        assert_eq!(listed.len(), 2);
        assert_eq!(listed[0].key, "assets/logs/a.txt");
        assert_eq!(listed[1].size, 2);
    }

    #[tokio::test]
    async fn list_rejects_limits_larger_than_gcs_accepts() {
        let store = test_store();
        let err = store
            .list("logs", Some(i32::MAX as usize + 1))
            .await
            .unwrap_err();
        assert_eq!(err.code(), ErrorCode::InvalidInput);
    }

    #[tokio::test]
    async fn presigned_url_reports_unsupported_operation() {
        let store = test_store();
        let err = store
            .presigned_url("input.txt", Duration::from_mins(1))
            .await
            .unwrap_err();
        assert_eq!(err.code(), ErrorCode::InvalidInput);
    }

    #[tokio::test]
    async fn copy_and_rename_compose_existing_operations() {
        let store = test_store();
        let copied = store.copy("input.txt", "output.txt").await.unwrap();
        assert_eq!(copied.key, "output.txt");
        assert_eq!(copied.size, 10);

        let renamed = store.rename("input.txt", "output.txt").await.unwrap();
        assert_eq!(renamed.key, "output.txt");
    }

    #[test]
    fn constructs_offline_without_building_transport() {
        let store = GcsStore::new(Config {
            bucket: "public-assets".into(),
            prefix: Some("uploads".into()),
            anonymous: true,
        });

        assert_eq!(store.full_key("image.png"), "uploads/image.png");
    }

    #[test]
    fn slash_only_prefix_is_ignored() {
        let store = GcsStore::new(Config {
            bucket: "public-assets".into(),
            prefix: Some("///".into()),
            anonymous: true,
        });

        assert_eq!(store.full_key("image.png"), "image.png");
    }

    #[test]
    fn register_adds_backend_without_constructing_client() {
        let mut registry = StorageRegistry::new();
        register(
            &mut registry,
            Config {
                bucket: "test".into(),
                prefix: None,
                anonymous: true,
            },
        )
        .unwrap();
        assert!(registry.contains("gcs"));
    }

    #[tokio::test]
    async fn factory_constructs_anonymous_store_from_config() {
        let factory = GcsFactory {
            config: Config {
                bucket: "public-assets".into(),
                prefix: Some("uploads".into()),
                anonymous: true,
            },
        };

        factory.create(&StorageConfig::default()).await.unwrap();
    }
}
