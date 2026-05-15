//! Axum transport details used by `rskit-server`.

#![warn(missing_docs)]

mod cors;
mod error;
mod extractors;
mod headers;
mod tenant;

pub use cors::CorsPolicy;
pub use error::{ErrorHandlerLayer, HttpError};
pub use extractors::{CorrelationId, RequestId};
pub use headers::{
    SecurityHeadersConfig, SecurityHeadersLayer, SecurityHeadersService, TransportSecurity,
};
pub use tenant::{
    TenantConfig, TenantId, set_tenant_in_extensions, tenant_from_extensions, tenant_middleware,
};
