use std::fmt;

use rskit_errors::AppResult;

/// Adapter contract for configuration sources.
///
/// Implement this trait in opt-in backend crates such as a future Vault, Parameter Store,
/// or remote-config adapter. `rskit-config` owns ordering, decoding, defaults, and validation;
/// adapters only return collected values.
pub trait ConfigSource: fmt::Debug + Send + Sync + 'static {
    /// Collect this source into a `config` source object.
    fn collect(&self) -> AppResult<config::Config>;
}
