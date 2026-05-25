//! Component shutdown result types.

use rskit_errors::AppError;

/// Detailed result for an attempted component stop.
#[derive(Debug)]
pub struct StopResult {
    /// Component name.
    pub name: String,
    /// Stop error, if one occurred.
    pub error: Option<AppError>,
}
