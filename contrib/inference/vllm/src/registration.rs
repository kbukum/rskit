use std::sync::Arc;

use rskit_inference::{Factory, Registry, RegistryError};

use crate::{Config, VLLM_KIND, VllmAdapter};

/// Explicitly register the vLLM adapter factory.
pub fn register(registry: &mut Registry, config: Config) -> Result<(), RegistryError> {
    let factory: Factory = Arc::new(move || Ok(Arc::new(VllmAdapter::new(config.clone())?)));
    registry.register(VLLM_KIND, factory)
}
