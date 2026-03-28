use async_trait::async_trait;

use rskit_errors::AppResult;

/// Policy enforcement point — checks whether a subject is allowed to perform
/// an action on a resource.
#[async_trait]
pub trait Checker: Send + Sync {
    /// Returns `Ok(())` when the action is permitted, or an
    /// [`AppError::forbidden`] when denied.
    async fn check(&self, subject: &str, action: &str, resource: &str) -> AppResult<()>;
}
