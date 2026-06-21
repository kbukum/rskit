//! `core-cli` — a CLI built entirely on rskit **core** crates.
//!
//! This library crate holds the command logic so it can be exercised by both
//! the binary (`main.rs`) and the integration tests. It depends only on
//! foundation crates (`rskit-config`, `rskit-logging`, `rskit-cli`,
//! `rskit-errors`, `rskit-version`) and links **no** transport/server crate —
//! see `docs/CONSUMER-CLASSES.md`.

pub mod commands;
pub mod settings;
