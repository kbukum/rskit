use std::sync::Arc;

use rskit_inference::{Factory, Registry, RegistryError};

use crate::{Config, TRITON_KIND, TritonInference};

/// Register the Triton adapter factory in an explicit registry.
pub fn register(registry: &mut Registry, config: Config) -> Result<(), RegistryError> {
    let factory: Factory = Arc::new(move || Ok(Arc::new(TritonInference::new(config.clone())?)));
    registry.register(TRITON_KIND, factory)
}
