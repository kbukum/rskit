use std::path::{Path, PathBuf};
use std::sync::Arc;

use rskit_codec::{Codec, TomlCodec, Value};
use rskit_errors::{AppError, AppResult};
use serde::de::DeserializeOwned;

use super::merge::IncludeMerge;

/// Maximum size of a single configuration file (1 MiB).
const MAX_CONFIG_BYTES: u64 = 1024 * 1024;

/// Strict, layered document loader.
///
/// Loads a canonical config file, optionally merging in include files, and
/// deserializes into a typed schema while honoring `#[serde(deny_unknown_fields)]`.
/// Unlike the [`crate::ConfigLoader`] pipeline (built on the `config` crate's
/// value tree), this path decodes through [`rskit_codec`] into the canonical
/// [`Value`] tree and deserializes from it, so serde's unknown-field rejection
/// fires and dynamic-keyed sections can be retained verbatim as
/// [`crate::strict::RawValue`] for downstream parsing.
///
/// The on-disk format is pluggable via [`Codec`]: TOML is the built-in default
/// ([`StrictLoader::new`]); any other codec (JSON, …) drops in through
/// [`StrictLoader::with_codec`].
///
/// Include files are treated as defaults: the canonical file's values win over
/// includes, and later includes win over earlier ones. Array-of-tables sections
/// registered as identity-keyed (see [`IncludeMerge`]) are concatenated and
/// hard-error on duplicate identity.
#[derive(Debug)]
pub struct StrictLoader {
    path: PathBuf,
    includes: Vec<PathBuf>,
    merge: IncludeMerge,
    codec: Arc<dyn Codec>,
}

impl StrictLoader {
    /// Create a loader for the canonical file at `path`, decoded as TOML.
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self {
            path: path.into(),
            includes: Vec::new(),
            merge: IncludeMerge::new(),
            codec: Arc::new(TomlCodec),
        }
    }

    /// Add an include file merged beneath the canonical file (a default source).
    #[must_use]
    pub fn with_include(mut self, path: impl Into<PathBuf>) -> Self {
        self.includes.push(path.into());
        self
    }

    /// Add multiple include files, in increasing precedence order.
    #[must_use]
    pub fn with_includes<I, P>(mut self, paths: I) -> Self
    where
        I: IntoIterator<Item = P>,
        P: Into<PathBuf>,
    {
        self.includes.extend(paths.into_iter().map(Into::into));
        self
    }

    /// Set the identity-aware include-merge configuration.
    #[must_use]
    pub fn with_merge(mut self, merge: IncludeMerge) -> Self {
        self.merge = merge;
        self
    }

    /// Set the [`Codec`] used to decode the canonical file and every include.
    ///
    /// Defaults to [`TomlCodec`]. Use this to load a strict JSON document, or any
    /// user-supplied format, without changing the loader.
    #[must_use]
    pub fn with_codec(mut self, codec: Arc<dyn Codec>) -> Self {
        self.codec = codec;
        self
    }

    /// Load, merge includes, and deserialize into `T`.
    ///
    /// Honors `#[serde(deny_unknown_fields)]`: an unknown key is a hard error.
    pub fn load<T>(&self) -> AppResult<T>
    where
        T: DeserializeOwned,
    {
        let value = self.load_raw()?;
        T::deserialize(value).map_err(|error| {
            AppError::invalid_input(
                "config",
                format!("failed to parse '{}': {error}", self.path.display()),
            )
        })
    }

    /// Load and merge includes into a single raw value tree (no typed schema).
    ///
    /// Validates identity-keyed sections but performs no schema typing, so
    /// callers can inspect or hand off dynamic-keyed subtrees verbatim.
    pub fn load_raw(&self) -> AppResult<Value> {
        let canonical = self.read(&self.path)?;
        self.assemble(self.includes.iter().map(PathBuf::as_path), canonical)
    }

    /// Load and deserialize into `T`, resolving include paths from the canonical
    /// document itself.
    ///
    /// Reads the canonical file exactly once, passes its decoded value tree to
    /// `resolve` to obtain the include list (for configs that declare their own
    /// includes, e.g. `[toven].include = [...]`), then merges those includes
    /// beneath the canonical document. Any statically-registered
    /// [`with_includes`](Self::with_includes) are ignored by this entry point.
    ///
    /// Honors `#[serde(deny_unknown_fields)]`.
    pub fn load_resolving_includes<T, F>(&self, resolve: F) -> AppResult<T>
    where
        T: DeserializeOwned,
        F: FnOnce(&Value) -> AppResult<Vec<PathBuf>>,
    {
        let value = self.load_raw_resolving_includes(resolve)?;
        T::deserialize(value).map_err(|error| {
            AppError::invalid_input(
                "config",
                format!("failed to parse '{}': {error}", self.path.display()),
            )
        })
    }

    /// Raw counterpart of [`load_resolving_includes`](Self::load_resolving_includes).
    ///
    /// Reads the canonical file once, derives the include list from it via
    /// `resolve`, and merges the includes beneath the canonical document.
    pub fn load_raw_resolving_includes<F>(&self, resolve: F) -> AppResult<Value>
    where
        F: FnOnce(&Value) -> AppResult<Vec<PathBuf>>,
    {
        let canonical = self.read(&self.path)?;
        let includes = resolve(&canonical)?;
        self.assemble(includes.iter().map(PathBuf::as_path), canonical)
    }

    /// Merge `includes` beneath an already-decoded `canonical` document.
    ///
    /// Includes are applied in increasing precedence (later wins over earlier),
    /// then the canonical document is merged on top so it wins every collision.
    fn assemble<'a>(
        &self,
        includes: impl Iterator<Item = &'a Path>,
        canonical: Value,
    ) -> AppResult<Value> {
        let mut document = Value::Object(serde_json::Map::new());
        for include in includes {
            let overlay = self.read(include)?;
            document = self.merge.merge(document, overlay)?;
        }
        document = self.merge.merge(document, canonical)?;
        self.merge.validate(&document)?;
        Ok(document)
    }

    /// Read and decode a single file into the canonical [`Value`] tree.
    fn read(&self, path: &Path) -> AppResult<Value> {
        let text = rskit_fs::sync_io::file::read_string_bounded(path, MAX_CONFIG_BYTES)?;
        self.codec.decode_value(&text).map_err(|error| {
            AppError::invalid_input("config", format!("failed to parse '{}'", path.display()))
                .with_cause(error)
        })
    }
}

/// Load a single strict file into `T` with no includes (decoded as TOML).
pub fn load_strict<T>(path: impl Into<PathBuf>) -> AppResult<T>
where
    T: DeserializeOwned,
{
    StrictLoader::new(path).load()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::strict::{IdentityKey, RawValue};
    use serde::Deserialize;
    use std::collections::BTreeMap;

    #[derive(Debug, Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Doc {
        name: String,
        #[serde(default)]
        ecosystems: BTreeMap<String, RawValue>,
    }

    fn write(dir: &Path, name: &str, body: &str) -> PathBuf {
        let path = dir.join(name);
        std::fs::write(&path, body).unwrap();
        path
    }

    #[test]
    fn load_strict_rejects_unknown_top_level_key() {
        let dir = tempfile::tempdir().unwrap();
        let path = write(dir.path(), "c.toml", "name = \"toven\"\nextra = true\n");

        let err = load_strict::<Doc>(&path).unwrap_err();

        assert!(err.to_string().contains("unknown field"));
    }

    #[test]
    fn load_retains_dynamic_keyed_subtree_verbatim() {
        let dir = tempfile::tempdir().unwrap();
        let path = write(
            dir.path(),
            "c.toml",
            "name = \"toven\"\n[ecosystems.rust]\nedition = 2024\nfeatures = [\"a\"]\n",
        );

        let doc: Doc = load_strict(&path).unwrap();

        let rust = doc.ecosystems.get("rust").unwrap();
        assert_eq!(rust.get("edition").unwrap().as_i64(), Some(2024));
        assert_eq!(rust.get("features").unwrap().as_array().unwrap().len(), 1);
    }

    #[test]
    fn includes_are_defaults_canonical_wins() {
        let dir = tempfile::tempdir().unwrap();
        let base = write(dir.path(), "base.toml", "name = \"base\"\n");
        let main = write(dir.path(), "c.toml", "name = \"main\"\n");

        let doc: Doc = StrictLoader::new(&main).with_include(&base).load().unwrap();

        assert_eq!(doc.name, "main");
    }

    #[test]
    fn resolving_includes_reads_canonical_once_and_merges_beneath() {
        let dir = tempfile::tempdir().unwrap();
        write(dir.path(), "extra.toml", "name = \"included\"\n");
        let main = write(
            dir.path(),
            "c.toml",
            "name = \"main\"\n[ecosystems.rust]\nedition = 2024\n",
        );

        let doc: Doc = StrictLoader::new(&main)
            .load_resolving_includes(|_canonical| Ok(vec![dir.path().join("extra.toml")]))
            .unwrap();

        // Canonical wins the scalar; the included default merges beneath.
        assert_eq!(doc.name, "main");
        assert!(doc.ecosystems.contains_key("rust"));
    }

    #[test]
    fn include_merge_rejects_duplicate_identity_across_files() {
        #[derive(Debug, Deserialize)]
        struct Groups {
            #[serde(default)]
            #[allow(dead_code)]
            groups: Vec<Group>,
        }
        #[derive(Debug, Deserialize)]
        struct Group {
            #[allow(dead_code)]
            name: String,
        }

        let dir = tempfile::tempdir().unwrap();
        let base = write(dir.path(), "base.toml", "[[groups]]\nname = \"dup\"\n");
        let main = write(dir.path(), "c.toml", "[[groups]]\nname = \"dup\"\n");

        let err = StrictLoader::new(&main)
            .with_include(&base)
            .with_merge(IncludeMerge::new().with_identity("groups", IdentityKey::new("name")))
            .load::<Groups>()
            .unwrap_err();

        assert!(err.to_string().contains("duplicate"));
    }

    #[test]
    fn invalid_toml_surfaces_typed_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = write(dir.path(), "c.toml", "name = \n");

        let err = load_strict::<Doc>(&path).unwrap_err();

        assert!(err.to_string().contains("failed to parse"));
    }
}
