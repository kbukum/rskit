//! Service-application config convenience trait.

use crate::ServiceConfig;

/// Trait that every application config struct must implement.
///
/// Typically implemented by a struct that embeds [`ServiceConfig`] and adds service-specific fields.
///
/// ```no_run
/// use rskit_config::{AppConfig, ConfigLoader, SecretString, ServiceConfig};
/// use rskit_validation::Validate;
///
/// #[derive(serde::Deserialize)]
/// struct MyConfig {
///     #[serde(flatten)]
///     service: ServiceConfig,
///     grpc_port: u16,
///     api_token: SecretString,
/// }
///
/// impl Validate for MyConfig {
///     fn validate(&self) -> Result<(), validator::ValidationErrors> {
///         self.service.validate()?;
///         if self.grpc_port == 0 {
///             let mut errors = validator::ValidationErrors::new();
///             errors.add("grpc_port", validator::ValidationError::new("range"));
///             return Err(errors);
///         }
///         Ok(())
///     }
/// }
///
/// impl AppConfig for MyConfig {
///     fn apply_defaults(&mut self) {
///         if self.grpc_port == 0 { self.grpc_port = 50051; }
///     }
///     fn service_config(&self) -> &ServiceConfig { &self.service }
/// }
///
/// # fn main() -> rskit_errors::AppResult<()> {
/// let cfg: MyConfig = ConfigLoader::app().load_app()?;
/// assert_eq!(cfg.api_token.to_string(), "***");
/// # Ok(())
/// # }
/// ```
pub trait AppConfig:
    serde::de::DeserializeOwned + rskit_validation::Validate + Send + Sync + 'static
{
    /// Apply any programmatic defaults after deserialization.
    fn apply_defaults(&mut self);
    /// Return a reference to the embedded [`ServiceConfig`].
    fn service_config(&self) -> &ServiceConfig;
}
