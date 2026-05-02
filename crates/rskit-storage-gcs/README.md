# rskit-storage-gcs

Google Cloud Storage adapter for `rskit-storage`.

`rskit-storage-gcs` is an opt-in backend crate. The core `rskit-storage`
crate contains the `FileStore` trait, `StorageRegistry`, and local filesystem
backend; this crate owns the Google Cloud Storage client dependency and
registers itself only when the application explicitly calls `register_gcs`.

## Installation

```toml
[dependencies]
rskit-storage = "0.1"
rskit-storage-gcs = "0.1"
```

## Usage

```rust,no_run
use rskit_storage::{StorageConfig, StorageRegistry};
use rskit_storage_gcs::{GcsStoreConfig, register_gcs};

# async fn example() -> rskit_errors::AppResult<()> {
let mut registry = StorageRegistry::new();
register_gcs(&mut registry)?;

let store = registry
    .build(&StorageConfig {
        backend: "gcs".into(),
        options: serde_json::to_value(GcsStoreConfig {
            bucket: "assets".into(),
            prefix: Some("uploads".into()),
        })?,
    })
    .await?;
# Ok(())
# }
```

Importing this crate has no side effects. Applications own the registry and
choose the backend through configuration.
