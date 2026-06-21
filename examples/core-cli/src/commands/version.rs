//! `version` subcommand: report foundation and application version info.

use rskit_cli::OutputKV;

/// Render version information drawn from `rskit-version` and this crate.
#[must_use]
pub fn render() -> OutputKV {
    let mut kv = OutputKV::new();
    kv.add("core-cli", env!("CARGO_PKG_VERSION"))
        .add("rskit-version", rskit_version::get_full_version());
    kv
}
