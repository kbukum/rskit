//! Pluggable structured-text codecs over a shared value tree.
//!
//! `rskit-codec` provides a small, object-safe [`Codec`] contract for encoding
//! and decoding structured text formats (TOML, JSON, …) through one canonical value model —
//! [`serde_json::Value`]. Any crate that reads or writes a config file, manifest,
//! or document reuses these codecs instead of re-implementing "bounded read → parse → typed error" per format.
//!
//! # Value model
//!
//! [`serde_json::Value`] is the canonical in-memory tree. It is format-neutral
//! and the de-facto Rust interchange type, so TOML and JSON both decode into it
//! and the [`value`] merge operates on it. One consequence:
//! types without a JSON equivalent (notably TOML datetimes) are not part of the model —
//! represent such values as strings.
//!
//! # Object safety
//!
//! [`Codec`] is object-safe (`Arc<dyn Codec>`)
//! so a codec can be selected at runtime (e.g. by file extension via [`select`]). The generic,
//! type-driven conveniences [`encode`] and [`decode`] are free functions taking `&dyn Codec`,
//! keeping the trait object-safe while still supporting `#[derive(Serialize, Deserialize)]` types.
//! `decode::<T>` honors `#[serde(deny_unknown_fields)]`.
//!
//! # Quick start
//!
//! ```
//! use rskit_codec::{JsonCodec, decode, encode};
//! use serde::{Deserialize, Serialize};
//!
//! #[derive(Serialize, Deserialize, PartialEq, Debug)]
//! struct Settings {
//!     name: String,
//!     retries: u8,
//! }
//!
//! let codec = JsonCodec::default();
//! let parsed: Settings = decode(&codec, r#"{ "name": "svc", "retries": 3 }"#).unwrap();
//! assert_eq!(parsed, Settings { name: "svc".into(), retries: 3 });
//!
//! let text = encode(&codec, &parsed).unwrap();
//! assert!(text.contains("\"name\""));
//! ```

#![warn(missing_docs)]

mod codec;
pub mod framing;
mod json;
pub mod select;
#[cfg(feature = "toml")]
mod toml;
pub mod value;

pub use codec::{Codec, decode, encode};
pub use json::{JsonCodec, JsonStyle};
#[cfg(feature = "toml")]
pub use toml::TomlCodec;

/// Re-export of the canonical value model.
pub use serde_json::Value;
