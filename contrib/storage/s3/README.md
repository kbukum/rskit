# rskit-storage-s3

Amazon S3 and S3-compatible storage adapter for `rskit-storage`.

`rskit-storage-s3` is an opt-in backend crate. The core `rskit-storage` crate
contains the `FileStore` trait, `StorageRegistry`, and local filesystem backend;
this crate owns the AWS SDK dependency and registers itself only when the
application explicitly calls `register`.

## Installation

```toml
[dependencies]
rskit-storage = "0.1"
rskit-storage-s3 = "0.1"
```

## Usage

```rust,no_run
use rskit_storage::{StorageConfig, StorageRegistry};
use rskit_storage_s3::{Config, register};

# async fn example() -> rskit_errors::AppResult<()> {
let mut registry = StorageRegistry::new();
register(
    &mut registry,
    Config {
        bucket: "assets".into(),
        region: Some("us-east-1".into()),
        endpoint: None,
        prefix: Some("uploads".into()),
        force_path_style: false,
        access_key_id: None,
        secret_access_key: None,
    },
)?;

let store = registry
    .build(&StorageConfig {
        backend: "s3".into(),
        ..Default::default()
    })
    .await?;
# Ok(())
# }
```

Importing this crate has no side effects. Applications own the registry and
choose the backend through configuration.
