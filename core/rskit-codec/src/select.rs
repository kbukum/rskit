//! Runtime codec selection by file extension or name.
//!
//! Returns an `Arc<dyn Codec>`
//! so a caller (a file sink, a document loader) can pick a codec at runtime from a path's extension without knowing the concrete type.
//! Only the codecs compiled in are selectable: the TOML codec requires the default-on `toml` feature;
//! JSON is always available.

use std::path::Path;
use std::sync::Arc;

use crate::JsonCodec;
#[cfg(feature = "toml")]
use crate::TomlCodec;
use crate::codec::Codec;

/// Return a codec for a lowercase format `name` (for example `"toml"`, `"json"`),
/// or `None` when no compiled-in codec matches.
#[must_use]
pub fn codec_for_name(name: &str) -> Option<Arc<dyn Codec>> {
    match name.to_ascii_lowercase().as_str() {
        #[cfg(feature = "toml")]
        "toml" => Some(Arc::new(TomlCodec)),
        "json" => Some(Arc::new(JsonCodec::default())),
        _ => None,
    }
}

/// Return a codec for `path`'s file extension, or `None` when the extension is missing or unrecognized.
#[must_use]
pub fn codec_for_path(path: &Path) -> Option<Arc<dyn Codec>> {
    let ext = path.extension().and_then(|ext| ext.to_str())?;
    codec_for_name(ext)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn selects_json_by_name_and_path() {
        assert_eq!(codec_for_name("json").unwrap().name(), "json");
        assert_eq!(codec_for_name("JSON").unwrap().name(), "json");
        assert_eq!(
            codec_for_path(Path::new("/etc/app/config.json"))
                .unwrap()
                .name(),
            "json"
        );
    }

    #[cfg(feature = "toml")]
    #[test]
    fn selects_toml_by_name_and_path() {
        assert_eq!(codec_for_name("toml").unwrap().name(), "toml");
        assert_eq!(
            codec_for_path(Path::new("config.toml")).unwrap().name(),
            "toml"
        );
    }

    #[test]
    fn returns_none_for_unknown_or_missing_extension() {
        assert!(codec_for_name("yaml").is_none());
        assert!(codec_for_path(Path::new("config")).is_none());
    }
}
