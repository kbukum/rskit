# rskit-storage-gcs

Google Cloud Storage backend for `rskit-storage`.

This crate implements `rskit_storage::store::FileStore` as an opt-in adapter.
Core `rskit-storage` stays focused on file utilities, local storage, and the
backend trait.

## Authentication

`GcsStore` uses Google application default credentials by default. It reads the
standard `GOOGLE_APPLICATION_CREDENTIALS`, `GOOGLE_APPLICATION_CREDENTIALS_JSON`,
or metadata-server sources supported by `google-cloud-storage`.

Set `GcsStoreConfig::anonymous` only for explicitly public buckets that require
unsigned requests.

## Usage

```toml
[dependencies]
rskit-storage = "0.1"
rskit-storage-gcs = "0.1"
```

```rust,no_run
use rskit_storage_gcs::{GcsStore, GcsStoreConfig};

# async fn example() -> rskit_errors::AppResult<()> {
let store = GcsStore::new(GcsStoreConfig {
    bucket: "assets".into(),
    prefix: Some("uploads".into()),
    anonymous: false,
})
.await?;
# Ok(())
# }
```
