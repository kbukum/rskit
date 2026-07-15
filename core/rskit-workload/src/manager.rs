//! Workload lifecycle manager and optional capability traits.
//!
//! Every backend implements [`Manager`]. Backends opt into extra capabilities
//! (exec, stats, log streaming, event watching) by implementing the focused
//! capability traits, mirroring gokit's optional provider interfaces.

use async_trait::async_trait;
use futures::stream::BoxStream;
use rskit_errors::AppResult;

use crate::report::{
    DeployResult, ExecResult, WaitResult, WorkloadEvent, WorkloadInfo, WorkloadStats,
    WorkloadStatus,
};
use crate::spec::{DeployRequest, ListFilter, LogOptions};

/// Core workload lifecycle operations. All backends implement this trait.
///
/// Every method takes an explicit workload identifier and is cancellation-aware
/// through the caller's async runtime; backends must apply their own timeouts to
/// remote calls.
#[async_trait]
pub trait Manager: Send + Sync {
    /// Create and start a workload.
    async fn deploy(&self, request: DeployRequest) -> AppResult<DeployResult>;

    /// Gracefully stop a running workload.
    async fn stop(&self, id: &str) -> AppResult<()>;

    /// Remove a stopped workload and clean up its resources.
    async fn remove(&self, id: &str) -> AppResult<()>;

    /// Stop and restart a workload.
    async fn restart(&self, id: &str) -> AppResult<()>;

    /// Return the current status of a workload.
    async fn status(&self, id: &str) -> AppResult<WorkloadStatus>;

    /// Block until the workload exits, returning its exit status.
    async fn wait(&self, id: &str) -> AppResult<WaitResult>;

    /// Return buffered log lines for a workload.
    async fn logs(&self, id: &str, options: LogOptions) -> AppResult<Vec<String>>;

    /// List workloads matching `filter`.
    async fn list(&self, filter: ListFilter) -> AppResult<Vec<WorkloadInfo>>;

    /// Verify the backend runtime is reachable and healthy.
    async fn health_check(&self) -> AppResult<()>;
}

/// Implemented by backends that can execute a command inside a running workload.
#[async_trait]
pub trait ExecCapable: Send + Sync {
    /// Execute `command` inside the workload identified by `id`.
    async fn exec(&self, id: &str, command: &[String]) -> AppResult<ExecResult>;
}

/// Implemented by backends that expose real-time resource usage statistics.
#[async_trait]
pub trait StatsCapable: Send + Sync {
    /// Return a resource usage snapshot for the workload identified by `id`.
    async fn stats(&self, id: &str) -> AppResult<WorkloadStats>;
}

/// Implemented by backends that can stream log lines in real time.
#[async_trait]
pub trait LogStreamer: Send + Sync {
    /// Stream log lines for the workload identified by `id`.
    async fn stream_logs(
        &self,
        id: &str,
        options: LogOptions,
    ) -> AppResult<BoxStream<'static, AppResult<String>>>;
}

/// Implemented by backends that can watch workload lifecycle events.
#[async_trait]
pub trait EventWatcher: Send + Sync {
    /// Stream lifecycle events for workloads matching `filter`.
    async fn watch_events(
        &self,
        filter: ListFilter,
    ) -> AppResult<BoxStream<'static, AppResult<WorkloadEvent>>>;
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use chrono::Utc;
    use futures::StreamExt;

    use super::*;
    use crate::state::WorkloadState;

    struct FakeManager;

    #[async_trait]
    impl Manager for FakeManager {
        async fn deploy(&self, request: DeployRequest) -> AppResult<DeployResult> {
            Ok(DeployResult {
                id: format!("{}-1", request.name),
                name: request.name,
                state: WorkloadState::Running,
            })
        }
        async fn stop(&self, _id: &str) -> AppResult<()> {
            Ok(())
        }
        async fn remove(&self, _id: &str) -> AppResult<()> {
            Ok(())
        }
        async fn restart(&self, _id: &str) -> AppResult<()> {
            Ok(())
        }
        async fn status(&self, id: &str) -> AppResult<WorkloadStatus> {
            Ok(WorkloadStatus {
                id: id.to_string(),
                state: WorkloadState::Running,
                running: true,
                ..Default::default()
            })
        }
        async fn wait(&self, _id: &str) -> AppResult<WaitResult> {
            Ok(WaitResult::default())
        }
        async fn logs(&self, _id: &str, _options: LogOptions) -> AppResult<Vec<String>> {
            Ok(vec!["line".to_string()])
        }
        async fn list(&self, _filter: ListFilter) -> AppResult<Vec<WorkloadInfo>> {
            Ok(Vec::new())
        }
        async fn health_check(&self) -> AppResult<()> {
            Ok(())
        }
    }

    #[async_trait]
    impl ExecCapable for FakeManager {
        async fn exec(&self, _id: &str, _command: &[String]) -> AppResult<ExecResult> {
            Ok(ExecResult {
                exit_code: 0,
                stdout: "ok".into(),
                stderr: String::new(),
            })
        }
    }

    #[async_trait]
    impl LogStreamer for FakeManager {
        async fn stream_logs(
            &self,
            _id: &str,
            _options: LogOptions,
        ) -> AppResult<BoxStream<'static, AppResult<String>>> {
            let lines = vec![Ok("a".to_string()), Ok("b".to_string())];
            Ok(futures::stream::iter(lines).boxed())
        }
    }

    #[async_trait]
    impl EventWatcher for FakeManager {
        async fn watch_events(
            &self,
            _filter: ListFilter,
        ) -> AppResult<BoxStream<'static, AppResult<WorkloadEvent>>> {
            let event = WorkloadEvent {
                id: "1".into(),
                name: "api".into(),
                event: "start".into(),
                timestamp: Utc::now(),
                message: String::new(),
            };
            Ok(futures::stream::iter(vec![Ok(event)]).boxed())
        }
    }

    #[tokio::test]
    async fn manager_is_object_safe_and_drives_lifecycle() {
        let manager: Arc<dyn Manager> = Arc::new(FakeManager);
        let result = manager
            .deploy(DeployRequest::new("api", "nginx"))
            .await
            .unwrap();
        assert_eq!(result.id, "api-1");
        assert!(manager.status(&result.id).await.unwrap().running);
        assert!(manager.health_check().await.is_ok());
        manager.stop(&result.id).await.unwrap();
        manager.remove(&result.id).await.unwrap();
    }

    #[tokio::test]
    async fn capability_traits_are_usable_as_trait_objects() {
        let exec: Arc<dyn ExecCapable> = Arc::new(FakeManager);
        assert_eq!(
            exec.exec("id", &["ls".to_string()])
                .await
                .unwrap()
                .exit_code,
            0
        );

        let streamer: Arc<dyn LogStreamer> = Arc::new(FakeManager);
        let lines: Vec<_> = streamer
            .stream_logs("id", LogOptions::default())
            .await
            .unwrap()
            .filter_map(|line| async move { line.ok() })
            .collect()
            .await;
        assert_eq!(lines, vec!["a".to_string(), "b".to_string()]);

        let watcher: Arc<dyn EventWatcher> = Arc::new(FakeManager);
        let events: Vec<_> = watcher
            .watch_events(ListFilter::default())
            .await
            .unwrap()
            .collect()
            .await;
        assert_eq!(events.len(), 1);
    }
}
