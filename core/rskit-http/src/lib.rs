//! Axum transport details used by `rskit-server`.

#![warn(missing_docs)]

mod error;
mod extractors;
mod tenant;

pub use error::{ErrorHandlerLayer, HttpError};
pub use extractors::{CorrelationId, RequestId};
pub use tenant::{
    TenantConfig, TenantId, set_tenant_in_extensions, tenant_from_extensions, tenant_middleware,
};
