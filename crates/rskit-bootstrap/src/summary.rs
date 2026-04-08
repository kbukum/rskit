use rskit_config::ServiceConfig;

/// Print a human-readable startup summary to the tracing output.
pub fn print_startup(cfg: &ServiceConfig, component_count: usize) {
    tracing::debug!(
        service   = %cfg.name,
        version   = %cfg.version,
        env       = %cfg.environment,
        components = component_count,
        "starting service"
    );
}
