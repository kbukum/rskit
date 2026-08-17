# rskit-storage-gcs

Google Cloud Storage adapter for `rskit-storage`.

`rskit-storage-gcs` is an opt-in backend crate. The core `rskit-storage` crate contains the `FileStore` trait, `StorageRegistry`, and local filesystem backend; this crate owns the Google Cloud Storage client dependency and registers itself only when the application explicitly calls `register`.

## Authentication

The registered adapter uses Google application default credentials by default. It reads the standard `GOOGLE_APPLICATION_CREDENTIALS`, `GOOGLE_APPLICATION_CREDENTIALS_JSON`, or metadata-server sources supported by `google-cloud-storage`.

Set `Config::anonymous` only for explicitly public buckets that require unsigned requests.

## Installation

```toml
[dependencies]
rskit-storage = "0.2.0-alpha.5"
rskit-storage-gcs = "0.2.0-alpha.5"
```

## Usage

```rust,no_run
use rskit_storage::{StorageConfig, StorageRegistry};
use rskit_storage_gcs::{Config, register};

# async fn example() -> rskit_errors::AppResult<()> {
let mut registry = StorageRegistry::new();
register(
    &mut registry,
    Config {
        bucket: "assets".into(),
        prefix: Some("uploads".into()),
        anonymous: false,
    },
)?;

let store = registry
    .build(&StorageConfig {
        backend: "gcs".into(),
        ..Default::default()
    })
    .await?;
# Ok(())
# }
```

Importing this crate has no side effects. Applications own the registry and choose the backend through configuration.
