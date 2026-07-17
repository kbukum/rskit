use std::sync::Arc;

use rskit_inference::{Factory, Registry, RegistryError};

use crate::{Config, TGI_KIND, TgiAdapter};

/// Explicitly register the TGI adapter factory.
pub fn register(registry: &mut Registry, config: Config) -> Result<(), RegistryError> {
    let factory: Factory = Arc::new(move || Ok(Arc::new(TgiAdapter::new(config.clone())?)));
    registry.register(TGI_KIND, factory)
}
