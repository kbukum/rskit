//! Shared HTTP transport policies and Tower adapters used by `rskit-server`.

#![warn(missing_docs)]

mod cors;
mod extractors;
mod headers;
mod status;
mod tenant;

pub use cors::CorsPolicy;
pub use extractors::{
    CorrelationId, RequestId, correlation_id_from_extensions, request_id_from_extensions,
    set_correlation_id, set_request_id,
};
pub use headers::{
    SecurityHeadersConfig, SecurityHeadersLayer, SecurityHeadersService, TransportSecurity,
};
pub use status::{
    HttpHeaders, HttpRequest, HttpResponse, HttpStatusCode, app_error_status, is_success_status,
    status_to_error_code,
};
pub use tenant::{TenantConfig, TenantId, set_tenant_in_extensions, tenant_from_extensions};
