use std::path::{Path, PathBuf};
use std::sync::Arc;

use rskit_codec::{Codec, TomlCodec, Value};
use rskit_errors::{AppError, AppResult};
use rskit_fs::sync_io::file;
use rskit_util::SecretString;

use super::{ConfigSink, ConfigTable};

/// Upper bound on the backing file size accepted on read (1 MiB).
///
/// A larger file is rejected rather than buffered unbounded.
const MAX_FILE_BYTES: u64 = 1024 * 1024;

/// Temp-file prefix used for atomic replacement.
const TEMP_PREFIX: &str = "config";

/// File-backed writable config store.
///
/// A reference [`ConfigSink`] that persists keys to a flat table on disk. The
/// on-disk representation is pluggable via [`Codec`]: TOML is the built-in
/// default ([`FileConfigSink::new`]), and any other format (JSON, …) drops in
/// through [`FileConfigSink::with_codec`] without changing the sink.
///
/// Filesystem access goes through `rskit-fs` (bounded reads + atomic
/// replacement), so a concurrent reader never observes a partial write.
///
/// Persisting writes the plaintext value to disk — this is the sink's explicit,
/// intended persistence. Protect the file with appropriate permissions; the
/// plaintext is never logged.
#[derive(Debug, Clone)]
pub struct FileConfigSink {
    path: PathBuf,
    codec: Arc<dyn Codec>,
}

impl FileConfigSink {
    /// Create a file sink backed by `path`, using the built-in TOML codec.
    ///
    /// The file is created on first write; a missing file reads as empty.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            codec: Arc::new(TomlCodec),
        }
    }

    /// Create a file sink backed by `path` with an explicit [`Codec`].
    ///
    /// Use this to persist as JSON or any user-supplied format.
    pub fn with_codec(path: impl Into<PathBuf>, codec: Arc<dyn Codec>) -> Self {
        Self {
            path: path.into(),
            codec,
        }
    }

    /// Return the backing file path.
    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Return the codec used to persist this sink.
    #[must_use]
    pub fn codec(&self) -> &dyn Codec {
        self.codec.as_ref()
    }

    /// Read the current table, treating a missing file as empty.
    fn read_table(&self) -> AppResult<ConfigTable> {
        if !file::exists(&self.path)? {
            return Ok(ConfigTable::new());
        }
        let contents = file::read_string_bounded(&self.path, MAX_FILE_BYTES)?;
        let value = self.codec.decode_value(&contents).map_err(|err| {
            AppError::invalid_input(
                "config",
                format!("failed to parse config file '{}'", self.path.display()),
            )
            .with_cause(err)
        })?;
        value_into_table(&value).ok_or_else(|| {
            AppError::invalid_input(
                "config",
                format!(
                    "config file '{}' must be a flat table of string values",
                    self.path.display()
                ),
            )
        })
    }

    /// Atomically replace the backing file with `table`.
    fn write_table(&self, table: &ConfigTable) -> AppResult<()> {
        let value = table_into_value(table);
        let rendered = self.codec.encode_value(&value).map_err(|err| {
            AppError::invalid_input(
                "config",
                format!("failed to encode config file '{}'", self.path.display()),
            )
            .with_cause(err)
        })?;
        file::write_atomic_replace(&self.path, rendered.as_bytes(), TEMP_PREFIX)
    }
}

/// Encode a flat table as a JSON object of string values.
fn table_into_value(table: &ConfigTable) -> Value {
    Value::Object(
        table
            .iter()
            .map(|(key, value)| (key.clone(), Value::String(value.clone())))
            .collect(),
    )
}

/// Decode a flat table from a value, requiring an object of string values.
fn value_into_table(value: &Value) -> Option<ConfigTable> {
    let object = value.as_object()?;
    let mut table = ConfigTable::new();
    for (key, entry) in object {
        table.insert(key.clone(), entry.as_str()?.to_string());
    }
    Some(table)
}

impl ConfigSink for FileConfigSink {
    fn set(&self, key: &str, value: SecretString) -> AppResult<()> {
        let mut table = self.read_table()?;
        table.insert(key.to_string(), value.expose().to_string());
        self.write_table(&table)
    }

    fn remove(&self, key: &str) -> AppResult<()> {
        let mut table = self.read_table()?;
        table.remove(key);
        self.write_table(&table)
    }

    fn set_many(&self, entries: Vec<(String, SecretString)>) -> AppResult<()> {
        let mut table = self.read_table()?;
        for (key, value) in entries {
            table.insert(key, value.expose().to_string());
        }
        self.write_table(&table)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn read_back(sink: &FileConfigSink, key: &str) -> Option<String> {
        sink.read_table().unwrap().get(key).cloned()
    }

    #[test]
    fn set_persists_and_survives_new_handle() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let sink = FileConfigSink::new(&path);
        sink.set("api_token", SecretString::new("s3cret")).unwrap();

        let reopened = FileConfigSink::new(&path);
        assert_eq!(read_back(&reopened, "api_token").as_deref(), Some("s3cret"));
    }

    #[test]
    fn missing_file_reads_as_empty() {
        let dir = tempdir().unwrap();
        let sink = FileConfigSink::new(dir.path().join("absent.toml"));
        assert!(sink.read_table().unwrap().is_empty());
        // Removing from an absent file is a no-op success.
        sink.remove("anything").unwrap();
    }

    #[test]
    fn remove_deletes_persisted_key() {
        let dir = tempdir().unwrap();
        let sink = FileConfigSink::new(dir.path().join("config.toml"));
        sink.set("a", SecretString::new("1")).unwrap();
        sink.set("b", SecretString::new("2")).unwrap();
        sink.remove("a").unwrap();
        assert!(read_back(&sink, "a").is_none());
        assert_eq!(read_back(&sink, "b").as_deref(), Some("2"));
    }

    #[test]
    fn set_many_persists_all_entries() {
        let dir = tempdir().unwrap();
        let sink = FileConfigSink::new(dir.path().join("config.toml"));
        sink.set_many(vec![
            ("a".to_string(), SecretString::new("1")),
            ("b".to_string(), SecretString::new("2")),
        ])
        .unwrap();
        assert_eq!(read_back(&sink, "a").as_deref(), Some("1"));
        assert_eq!(read_back(&sink, "b").as_deref(), Some("2"));
    }

    #[test]
    fn keys_with_dots_round_trip() {
        let dir = tempdir().unwrap();
        let sink = FileConfigSink::new(dir.path().join("config.toml"));
        sink.set("a.b.c", SecretString::new("v")).unwrap();
        assert_eq!(read_back(&sink, "a.b.c").as_deref(), Some("v"));
    }

    #[test]
    fn oversized_file_is_rejected() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("big.toml");
        std::fs::write(&path, vec![b'#'; (MAX_FILE_BYTES + 1) as usize]).unwrap();
        let sink = FileConfigSink::new(&path);
        let err = sink.set("k", SecretString::new("v")).unwrap_err();
        assert!(err.to_string().contains("exceeding limit"));
    }

    #[test]
    fn custom_codec_is_used_for_persistence() {
        // A trivial line-based codec proves the sink is format-neutral.
        #[derive(Debug)]
        struct LineCodec;

        impl Codec for LineCodec {
            fn name(&self) -> &'static str {
                "lines"
            }

            fn encode_value(&self, value: &Value) -> AppResult<String> {
                let object = value
                    .as_object()
                    .ok_or_else(|| AppError::invalid_input("config", "expected an object"))?;
                let mut out = String::new();
                for (k, v) in object {
                    let v = v
                        .as_str()
                        .ok_or_else(|| AppError::invalid_input("config", "expected a string"))?;
                    out.push_str(k);
                    out.push('=');
                    out.push_str(v);
                    out.push('\n');
                }
                Ok(out)
            }

            fn decode_value(&self, contents: &str) -> AppResult<Value> {
                let mut object = serde_json::Map::new();
                for line in contents.lines().filter(|l| !l.is_empty()) {
                    let (k, v) = line
                        .split_once('=')
                        .ok_or_else(|| AppError::invalid_input("config", "malformed line entry"))?;
                    object.insert(k.to_string(), Value::String(v.to_string()));
                }
                Ok(Value::Object(object))
            }
        }

        let dir = tempdir().unwrap();
        let path = dir.path().join("config.lines");
        let sink = FileConfigSink::with_codec(&path, Arc::new(LineCodec));
        sink.set("k", SecretString::new("v")).unwrap();

        let raw = std::fs::read_to_string(&path).unwrap();
        assert_eq!(raw, "k=v\n");
        assert_eq!(sink.codec().name(), "lines");

        let reopened = FileConfigSink::with_codec(&path, Arc::new(LineCodec));
        assert_eq!(read_back(&reopened, "k").as_deref(), Some("v"));
    }
}
