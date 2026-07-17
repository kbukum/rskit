//! Supabase Storage backend implementing [`rskit_storage::store::FileStore`].

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use rskit_errors::{AppError, AppResult, ErrorCode};
use rskit_storage::FileSource;
use rskit_storage::store::{
    FileStore, ProgressCallback, StorageConfig, StorageFactory, StorageRegistry, StoredFile,
    content_type_or_default, prefixed_key,
};
use rskit_util::SecretString;
use serde::{Deserialize, Serialize};
use url::Url;

/// Configuration for the Supabase Storage REST backend.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct Config {
    /// Supabase project URL or storage API base URL.
    pub endpoint: String,
    /// Storage bucket name.
    pub bucket: String,
    /// Key prefix applied to all objects.
    pub prefix: Option<String>,
    /// Anon or service-role token sent in `Authorization` and `apikey` headers.
    pub token: SecretString,
    /// Per-request timeout for every remote call.
    #[serde(default = "default_timeout")]
    pub timeout: Duration,
}

const fn default_timeout() -> Duration {
    Duration::from_secs(30)
}

/// Supabase Storage REST client implementing [`FileStore`].
pub struct SupabaseStore {
    config: Config,
    client: reqwest::Client,
    base_url: Url,
}

impl SupabaseStore {
    /// Create a store with a default [`reqwest::Client`].
    pub fn new(config: Config) -> AppResult<Self> {
        Self::new_with_client(config, reqwest::Client::new())
    }

    /// Create a store with an injected [`reqwest::Client`].
    pub fn new_with_client(config: Config, client: reqwest::Client) -> AppResult<Self> {
        validate_config(&config)?;
        let base_url = storage_base_url(&config.endpoint)?;
        Ok(Self {
            config,
            client,
            base_url,
        })
    }

    fn full_key(&self, key: &str) -> String {
        prefixed_key(self.config.prefix.as_deref(), key)
    }

    fn object_url(&self, key: &str) -> AppResult<Url> {
        join_segments(&self.base_url, &["object", &self.config.bucket, key])
    }

    fn list_url(&self) -> AppResult<Url> {
        join_segments(&self.base_url, &["object", "list", &self.config.bucket])
    }

    fn action_url(&self, action: &str) -> AppResult<Url> {
        join_segments(&self.base_url, &["object", action])
    }

    fn signed_url(&self, key: &str) -> AppResult<Url> {
        join_segments(
            &self.base_url,
            &["object", "sign", &self.config.bucket, key],
        )
    }

    fn authed(&self, request: reqwest::RequestBuilder) -> reqwest::RequestBuilder {
        request
            .bearer_auth(self.config.token.expose())
            .header("apikey", self.config.token.expose())
            .timeout(self.config.timeout)
    }

    async fn send(
        &self,
        request: reqwest::RequestBuilder,
        operation: &str,
    ) -> AppResult<reqwest::Response> {
        let response = request.send().await.map_err(|error| {
            AppError::new(
                send_error_code(&error),
                format!("Supabase {operation} request failed"),
            )
            .with_cause(error)
        })?;
        if response.status().is_success() {
            return Ok(response);
        }
        let status = response.status();
        let message = match response.text().await {
            Ok(body) => body,
            Err(error) => format!("<failed to read response body: {error}>"),
        };
        Err(AppError::new(
            status_to_error_code(status),
            format!("Supabase {operation} failed with status {status}: {message}"),
        ))
    }
}

#[async_trait::async_trait]
impl FileStore for SupabaseStore {
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
        let mut request = self
            .client
            .post(self.object_url(&full_key)?)
            .header("content-type", content_type_or_default(content_type))
            .body(data);
        if let Some(metadata) = &metadata {
            request = request.header(
                "x-metadata",
                serde_json::to_string(metadata).map_err(|error| {
                    AppError::new(
                        ErrorCode::InvalidInput,
                        "Supabase metadata must be JSON serializable",
                    )
                    .with_cause(error)
                })?,
            );
        }
        self.send(self.authed(request), "upload").await?;
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
        let response = self
            .send(
                self.authed(self.client.get(self.object_url(&full_key)?)),
                "download",
            )
            .await?;
        let data = response.bytes().await.map_err(|error| {
            AppError::new(
                ErrorCode::ExternalService,
                "Supabase download body read failed",
            )
            .with_cause(error)
        })?;
        Ok(FileSource::Bytes(data))
    }

    async fn delete(&self, key: &str) -> AppResult<()> {
        let full_key = self.full_key(key);
        let body = serde_json::json!({ "prefixes": [full_key] });
        self.send(
            self.authed(
                self.client
                    .delete(join_segments(
                        &self.base_url,
                        &["object", &self.config.bucket],
                    )?)
                    .json(&body),
            ),
            "delete",
        )
        .await?;
        Ok(())
    }

    async fn exists(&self, key: &str) -> AppResult<bool> {
        match self.head(key).await {
            Ok(_) => Ok(true),
            Err(error) if error.code() == ErrorCode::NotFound => Ok(false),
            Err(error) => Err(error),
        }
    }

    async fn head(&self, key: &str) -> AppResult<StoredFile> {
        let full_key = self.full_key(key);
        let response = self
            .send(
                self.authed(self.client.head(self.object_url(&full_key)?)),
                "head",
            )
            .await?;
        let size = response
            .headers()
            .get(reqwest::header::CONTENT_LENGTH)
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.parse::<u64>().ok())
            .unwrap_or(0);
        let content_type = response
            .headers()
            .get(reqwest::header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok());
        Ok(StoredFile::new(prefixed_key(None, key), size, content_type))
    }

    async fn list(&self, prefix: &str, limit: Option<usize>) -> AppResult<Vec<StoredFile>> {
        let mut body = serde_json::json!({ "prefix": self.full_key(prefix) });
        if let Some(limit) = limit {
            body["limit"] = serde_json::json!(limit);
        }
        let response = self
            .send(
                self.authed(self.client.post(self.list_url()?).json(&body)),
                "list",
            )
            .await?;
        let objects: Vec<SupabaseObject> = response.json().await.map_err(|error| {
            AppError::new(
                ErrorCode::ExternalService,
                "Supabase list response decode failed",
            )
            .with_cause(error)
        })?;
        Ok(objects.into_iter().map(StoredFile::from).collect())
    }

    async fn presigned_url(&self, key: &str, expires_in: Duration) -> AppResult<String> {
        if expires_in.is_zero() {
            return Err(AppError::new(
                ErrorCode::InvalidInput,
                "Supabase presign expires_in must be greater than zero",
            ));
        }
        let full_key = self.full_key(key);
        let expires_in_secs = expires_in.as_secs().max(1);
        let body = serde_json::json!({ "expiresIn": expires_in_secs });
        let response = self
            .send(
                self.authed(self.client.post(self.signed_url(&full_key)?).json(&body)),
                "presign",
            )
            .await?;
        let signed: SignedUrl = response.json().await.map_err(|error| {
            AppError::new(
                ErrorCode::ExternalService,
                "Supabase presign response decode failed",
            )
            .with_cause(error)
        })?;
        Ok(signed.signed_url)
    }

    async fn copy(&self, from_key: &str, to_key: &str) -> AppResult<StoredFile> {
        let full_from = self.full_key(from_key);
        let full_to = self.full_key(to_key);
        let body = serde_json::json!({
            "bucketId": self.config.bucket,
            "sourceKey": full_from,
            "destinationKey": full_to,
        });
        self.send(
            self.authed(self.client.post(self.action_url("copy")?).json(&body)),
            "copy",
        )
        .await?;
        self.head(to_key).await
    }

    async fn rename(&self, from_key: &str, to_key: &str) -> AppResult<StoredFile> {
        let full_from = self.full_key(from_key);
        let full_to = self.full_key(to_key);
        let body = serde_json::json!({
            "bucketId": self.config.bucket,
            "sourceKey": full_from,
            "destinationKey": full_to,
        });
        self.send(
            self.authed(self.client.post(self.action_url("move")?).json(&body)),
            "move",
        )
        .await?;
        self.head(to_key).await
    }
}

#[derive(Deserialize)]
struct SupabaseObject {
    name: String,
    #[serde(default)]
    metadata: SupabaseObjectMetadata,
}

#[derive(Default, Deserialize)]
struct SupabaseObjectMetadata {
    #[serde(default)]
    size: u64,
    #[serde(default, rename = "mimetype")]
    mime_type: Option<String>,
}

impl From<SupabaseObject> for StoredFile {
    fn from(value: SupabaseObject) -> Self {
        Self::new(
            value.name,
            value.metadata.size,
            value.metadata.mime_type.as_deref(),
        )
    }
}

#[derive(Deserialize)]
struct SignedUrl {
    #[serde(rename = "signedURL", alias = "signedUrl")]
    signed_url: String,
}

fn validate_config(config: &Config) -> AppResult<()> {
    if config.bucket.trim().is_empty() {
        return Err(AppError::new(
            ErrorCode::MissingField,
            "Supabase bucket is required",
        ));
    }
    if config.token.expose().trim().is_empty() {
        return Err(AppError::new(
            ErrorCode::MissingField,
            "Supabase token is required",
        ));
    }
    if config.timeout.is_zero() {
        return Err(AppError::new(
            ErrorCode::InvalidInput,
            "Supabase timeout must be greater than zero",
        ));
    }
    Ok(())
}

fn storage_base_url(endpoint: &str) -> AppResult<Url> {
    let mut url = Url::parse(endpoint).map_err(|error| {
        AppError::new(
            ErrorCode::InvalidInput,
            "Supabase endpoint must be a valid URL",
        )
        .with_cause(error)
    })?;
    if !url.path().trim_end_matches('/').ends_with("/storage/v1") {
        url = url.join("storage/v1/").map_err(|error| {
            AppError::new(
                ErrorCode::InvalidInput,
                "Supabase storage URL could not be built",
            )
            .with_cause(error)
        })?;
    }
    let normalized = url.path().trim_end_matches('/').to_owned();
    url.set_path(&normalized);
    url.set_query(None);
    Ok(url)
}

fn join_segments(base: &Url, segments: &[&str]) -> AppResult<Url> {
    let mut url = base.clone();
    {
        let mut path = url.path_segments_mut().map_err(|()| {
            AppError::new(
                ErrorCode::InvalidInput,
                "Supabase endpoint must be a base URL",
            )
        })?;
        for segment in segments {
            for part in segment.split('/').filter(|part| !part.is_empty()) {
                path.push(part);
            }
        }
    }
    Ok(url)
}

/// Classify a reqwest transport failure, preserving typed timeout and
/// connection information instead of flattening everything to
/// [`ErrorCode::ExternalService`].
fn send_error_code(error: &reqwest::Error) -> ErrorCode {
    if error.is_timeout() {
        ErrorCode::Timeout
    } else if error.is_connect() {
        ErrorCode::ConnectionFailed
    } else {
        ErrorCode::ExternalService
    }
}

/// Map an HTTP response status to an [`ErrorCode`], mirroring the canonical
/// mapping used across rskit so auth failures stay non-retryable while
/// genuinely transient statuses remain retryable.
const fn status_to_error_code(status: reqwest::StatusCode) -> ErrorCode {
    match status.as_u16() {
        400 => ErrorCode::InvalidInput,
        401 => ErrorCode::Unauthorized,
        403 => ErrorCode::Forbidden,
        404 => ErrorCode::NotFound,
        409 => ErrorCode::Conflict,
        429 => ErrorCode::RateLimited,
        503 => ErrorCode::ServiceUnavailable,
        504 => ErrorCode::Timeout,
        _ => ErrorCode::ExternalService,
    }
}

struct SupabaseFactory {
    config: Config,
}

#[async_trait::async_trait]
impl StorageFactory for SupabaseFactory {
    async fn create(&self, _config: &StorageConfig) -> AppResult<Arc<dyn FileStore>> {
        Ok(Arc::new(SupabaseStore::new(self.config.clone())?))
    }
}

/// Explicitly register the Supabase Storage backend.
pub fn register(registry: &mut StorageRegistry, config: Config) -> AppResult<()> {
    registry.register("supabase", Arc::new(SupabaseFactory { config }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use rskit_storage::store::StorageRegistry;
    use wiremock::matchers::{header, method, path, query_param_is_missing};
    use wiremock::{Mock, MockServer, ResponseTemplate};

    fn test_store(server: &MockServer) -> SupabaseStore {
        SupabaseStore::new_with_client(
            Config {
                endpoint: server.uri(),
                bucket: "assets".into(),
                prefix: Some("uploads".into()),
                token: SecretString::new("service-token"),
                timeout: Duration::from_secs(5),
            },
            reqwest::Client::new(),
        )
        .unwrap()
    }

    #[tokio::test]
    async fn upload_sends_token_in_header_and_never_query() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/storage/v1/object/assets/uploads/file.txt"))
            .and(header("authorization", "Bearer service-token"))
            .and(query_param_is_missing("access_token"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"Key":"uploads/file.txt"})),
            )
            .mount(&server)
            .await;

        let stored = test_store(&server)
            .upload(
                &FileSource::from_bytes(bytes::Bytes::from_static(b"payload")),
                "file.txt",
                Some("text/plain"),
                None,
            )
            .await
            .unwrap();
        assert_eq!(stored.key, "file.txt");
        assert_eq!(stored.size, 7);
    }

    #[tokio::test]
    async fn download_delete_head_list_and_presign_use_rest_api() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/storage/v1/object/assets/uploads/file.txt"))
            .respond_with(ResponseTemplate::new(200).set_body_bytes(b"payload".to_vec()))
            .mount(&server)
            .await;
        Mock::given(method("HEAD"))
            .and(path("/storage/v1/object/assets/uploads/file.txt"))
            .respond_with(
                ResponseTemplate::new(200)
                    .insert_header("content-length", "7")
                    .insert_header("content-type", "text/plain"),
            )
            .mount(&server)
            .await;
        Mock::given(method("POST")).and(path("/storage/v1/object/list/assets"))
            .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([{ "name": "uploads/file.txt", "metadata": {"size": 7, "mimetype": "text/plain"}}]))).mount(&server).await;
        Mock::given(method("POST"))
            .and(path("/storage/v1/object/sign/assets/uploads/file.txt"))
            .respond_with(
                ResponseTemplate::new(200)
                    .set_body_json(serde_json::json!({"signedURL":"/signed"})),
            )
            .mount(&server)
            .await;
        Mock::given(method("DELETE"))
            .and(path("/storage/v1/object/assets"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;
        let store = test_store(&server);
        assert_eq!(
            store
                .download("file.txt")
                .await
                .unwrap()
                .read_all()
                .await
                .unwrap()
                .as_ref(),
            b"payload"
        );
        assert!(store.exists("file.txt").await.unwrap());
        assert_eq!(
            store.head("file.txt").await.unwrap().content_type,
            "text/plain"
        );
        assert_eq!(store.list("", Some(10)).await.unwrap()[0].size, 7);
        assert_eq!(
            store
                .presigned_url("file.txt", Duration::from_mins(1))
                .await
                .unwrap(),
            "/signed"
        );
        store.delete("file.txt").await.unwrap();
    }

    #[tokio::test]
    async fn presign_rejects_zero_duration() {
        let server = MockServer::start().await;
        let store = test_store(&server);
        let err = store
            .presigned_url("file.txt", Duration::ZERO)
            .await
            .unwrap_err();
        assert_eq!(err.code(), ErrorCode::InvalidInput);
    }

    #[test]
    fn config_debug_redacts_token_and_validation_is_typed() {
        let cfg = Config {
            endpoint: "https://example.supabase.co".into(),
            bucket: "assets".into(),
            prefix: None,
            token: SecretString::new("secret"),
            timeout: Duration::from_secs(1),
        };
        let debug = format!("{cfg:?}");
        assert!(debug.contains("***"));
        assert!(!debug.contains("secret"));
        let serialized = serde_json::to_string(&cfg).unwrap();
        assert!(!serialized.contains("secret"));
        assert_eq!(
            validate_config(&Config {
                token: SecretString::default(),
                ..cfg
            })
            .unwrap_err()
            .code(),
            ErrorCode::MissingField
        );
    }

    #[test]
    fn register_is_explicit_and_duplicate_safe() {
        let mut registry = StorageRegistry::new();
        let cfg = Config {
            endpoint: "https://example.supabase.co".into(),
            bucket: "assets".into(),
            prefix: None,
            token: SecretString::new("token"),
            timeout: Duration::from_secs(5),
        };
        register(&mut registry, cfg.clone()).unwrap();
        assert!(registry.contains("supabase"));
        assert_eq!(
            register(&mut registry, cfg).unwrap_err().code(),
            ErrorCode::AlreadyExists
        );
    }

    #[test]
    fn status_mapping_keeps_auth_failures_non_retryable() {
        for (status, expected) in [
            (400, ErrorCode::InvalidInput),
            (401, ErrorCode::Unauthorized),
            (403, ErrorCode::Forbidden),
            (404, ErrorCode::NotFound),
            (409, ErrorCode::Conflict),
            (429, ErrorCode::RateLimited),
            (503, ErrorCode::ServiceUnavailable),
            (504, ErrorCode::Timeout),
            (500, ErrorCode::ExternalService),
        ] {
            let code = status_to_error_code(reqwest::StatusCode::from_u16(status).unwrap());
            assert_eq!(code, expected, "status {status}");
        }
        assert!(!status_to_error_code(reqwest::StatusCode::UNAUTHORIZED).is_retryable());
        assert!(!status_to_error_code(reqwest::StatusCode::FORBIDDEN).is_retryable());
    }

    #[tokio::test]
    async fn unauthorized_response_maps_to_non_retryable_error() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/storage/v1/object/assets/uploads/file.txt"))
            .respond_with(ResponseTemplate::new(401).set_body_string("invalid token"))
            .mount(&server)
            .await;
        let err = test_store(&server).download("file.txt").await.unwrap_err();
        assert_eq!(err.code(), ErrorCode::Unauthorized);
        assert!(!err.code().is_retryable());
    }

    #[tokio::test]
    async fn connect_failure_maps_to_connection_failed() {
        // Nothing is listening on this port, so the send fails with a connect error.
        let store = SupabaseStore::new_with_client(
            Config {
                endpoint: "http://127.0.0.1:1".into(),
                bucket: "assets".into(),
                prefix: None,
                token: SecretString::new("token"),
                timeout: Duration::from_secs(5),
            },
            reqwest::Client::new(),
        )
        .unwrap();
        let err = store.download("file.txt").await.unwrap_err();
        assert_eq!(err.code(), ErrorCode::ConnectionFailed);
    }

    #[test]
    fn config_applies_default_timeout_when_omitted() {
        let config: Config = serde_json::from_value(serde_json::json!({
            "endpoint": "https://example.supabase.co",
            "bucket": "assets",
            "prefix": null,
            "token": "token",
        }))
        .unwrap();
        assert_eq!(config.timeout, default_timeout());
    }

    #[test]
    fn new_builds_store_with_default_client() {
        let store = SupabaseStore::new(Config {
            endpoint: "https://example.supabase.co".into(),
            bucket: "assets".into(),
            prefix: None,
            token: SecretString::new("token"),
            timeout: Duration::from_secs(5),
        });
        assert!(store.is_ok());
    }

    #[test]
    fn new_rejects_invalid_endpoint_url() {
        let result = SupabaseStore::new(Config {
            endpoint: "not a url".into(),
            bucket: "assets".into(),
            prefix: None,
            token: SecretString::new("token"),
            timeout: Duration::from_secs(5),
        });
        assert_eq!(
            result.err().map(|error| error.code()),
            Some(ErrorCode::InvalidInput)
        );
    }

    #[test]
    fn validate_rejects_empty_bucket_and_zero_timeout() {
        let base = Config {
            endpoint: "https://example.supabase.co".into(),
            bucket: "assets".into(),
            prefix: None,
            token: SecretString::new("token"),
            timeout: Duration::from_secs(5),
        };
        assert_eq!(
            validate_config(&Config {
                bucket: "  ".into(),
                ..base.clone()
            })
            .unwrap_err()
            .code(),
            ErrorCode::MissingField
        );
        assert_eq!(
            validate_config(&Config {
                timeout: Duration::ZERO,
                ..base
            })
            .unwrap_err()
            .code(),
            ErrorCode::InvalidInput
        );
    }

    #[tokio::test]
    async fn upload_with_progress_delegates_to_upload() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/storage/v1/object/assets/uploads/file.txt"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;
        let stored = test_store(&server)
            .upload_with_progress(
                &FileSource::from_bytes(bytes::Bytes::from_static(b"payload")),
                "file.txt",
                Some("text/plain"),
                std::sync::Arc::new(|_| {}),
            )
            .await
            .unwrap();
        assert_eq!(stored.size, 7);
    }

    #[tokio::test]
    async fn upload_sends_metadata_header() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/storage/v1/object/assets/uploads/file.txt"))
            .and(header("x-metadata", "{\"owner\":\"alice\"}"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;
        let mut metadata = HashMap::new();
        metadata.insert("owner".to_owned(), "alice".to_owned());
        let stored = test_store(&server)
            .upload(
                &FileSource::from_bytes(bytes::Bytes::from_static(b"payload")),
                "file.txt",
                None,
                Some(metadata),
            )
            .await
            .unwrap();
        assert_eq!(
            stored.metadata.get("owner").map(String::as_str),
            Some("alice")
        );
    }

    #[tokio::test]
    async fn copy_and_rename_use_rest_api() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/storage/v1/object/copy"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;
        Mock::given(method("POST"))
            .and(path("/storage/v1/object/move"))
            .respond_with(ResponseTemplate::new(200))
            .mount(&server)
            .await;
        Mock::given(method("HEAD"))
            .and(path("/storage/v1/object/assets/uploads/dest.txt"))
            .respond_with(ResponseTemplate::new(200).insert_header("content-length", "3"))
            .mount(&server)
            .await;
        let store = test_store(&server);
        assert_eq!(store.copy("src.txt", "dest.txt").await.unwrap().size, 3);
        assert_eq!(store.rename("src.txt", "dest.txt").await.unwrap().size, 3);
    }

    #[tokio::test]
    async fn list_decode_failure_maps_to_external_service() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/storage/v1/object/list/assets"))
            .respond_with(ResponseTemplate::new(200).set_body_string("not json"))
            .mount(&server)
            .await;
        let err = test_store(&server).list("", None).await.unwrap_err();
        assert_eq!(err.code(), ErrorCode::ExternalService);
    }

    #[tokio::test]
    async fn presign_decode_failure_maps_to_external_service() {
        let server = MockServer::start().await;
        Mock::given(method("POST"))
            .and(path("/storage/v1/object/sign/assets/uploads/file.txt"))
            .respond_with(ResponseTemplate::new(200).set_body_string("not json"))
            .mount(&server)
            .await;
        let err = test_store(&server)
            .presigned_url("file.txt", Duration::from_mins(1))
            .await
            .unwrap_err();
        assert_eq!(err.code(), ErrorCode::ExternalService);
    }

    #[tokio::test]
    async fn slow_response_maps_to_timeout_error() {
        let server = MockServer::start().await;
        Mock::given(method("GET"))
            .and(path("/storage/v1/object/assets/uploads/file.txt"))
            .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_millis(500)))
            .mount(&server)
            .await;
        let store = SupabaseStore::new_with_client(
            Config {
                endpoint: server.uri(),
                bucket: "assets".into(),
                prefix: Some("uploads".into()),
                token: SecretString::new("token"),
                timeout: Duration::from_millis(20),
            },
            reqwest::Client::new(),
        )
        .unwrap();
        let err = store.download("file.txt").await.unwrap_err();
        assert_eq!(err.code(), ErrorCode::Timeout);
    }

    #[tokio::test]
    async fn factory_builds_store_through_registry() {
        let mut registry = StorageRegistry::new();
        register(
            &mut registry,
            Config {
                endpoint: "https://example.supabase.co".into(),
                bucket: "assets".into(),
                prefix: None,
                token: SecretString::new("token"),
                timeout: Duration::from_secs(5),
            },
        )
        .unwrap();
        let built = registry
            .build(&StorageConfig {
                backend: "supabase".into(),
                ..StorageConfig::default()
            })
            .await;
        assert!(built.is_ok());
    }
}
