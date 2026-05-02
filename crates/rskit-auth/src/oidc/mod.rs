//! OpenID Connect (OIDC) support — discovery, PKCE, token validation, and userinfo.

mod client;
mod config;
mod error;
mod http;
mod types;

pub use client::{OidcClient, validate_id_token};
pub use config::{OidcClientType, OidcConfig};
pub use error::OidcError;
pub use http::{OidcHttpClient, ReqwestOidcHttpClient};
pub use types::{
    OidcAuthorizationRequest, OidcClaims, OidcProviderMetadata, OidcTokenExchangeRequest,
    OidcUserInfo, PkcePair,
};
