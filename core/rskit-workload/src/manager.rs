//! Workload lifecycle manager and optional capability traits.
//!
//! Every backend implements [`Manager`]. Backends opt into extra capabilities
//! (exec, stats, log streaming, event watching) by implementing the focused
//! capability traits, mirroring gokit's optional provider interfaces.

use async_trait::async_trait;
use futures::stream::BoxStream;
use rskit_errors::AppResult;

use crate::report::{
    DeployResult, DiskUsage, ExecResult, ImageDetail, ImageEvent, SystemInfo, WaitResult,
    WorkloadEvent, WorkloadInfo, WorkloadStats, WorkloadStatus,
};
use crate::spec::{DeployRequest, ImageEventFilter, ListFilter, LogOptions};

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

/// Implemented by backends that report host-level system information.
#[async_trait]
pub trait SystemInfoCapable: Send + Sync {
    /// Return host information for the backend runtime.
    async fn system_info(&self) -> AppResult<SystemInfo>;
}

/// Implemented by backends that report runtime disk usage.
#[async_trait]
pub trait DiskUsageCapable: Send + Sync {
    /// Return disk usage for the backend runtime, broken down by category.
    async fn disk_usage(&self) -> AppResult<DiskUsage>;
}

/// Implemented by backends that can inspect image metadata.
#[async_trait]
pub trait ImageInspector: Send + Sync {
    /// Return detailed metadata for the image identified by `reference`.
    async fn image_inspect(&self, reference: &str) -> AppResult<ImageDetail>;
}

/// Implemented by backends that can watch image lifecycle events.
#[async_trait]
pub trait ImageEventWatcher: Send + Sync {
    /// Stream image lifecycle events matching `filter`.
    async fn watch_image_events(
        &self,
        filter: ImageEventFilter,
    ) -> AppResult<BoxStream<'static, AppResult<ImageEvent>>>;
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use futures::StreamExt;

    use super::*;
    use crate::test_support::FakeManager;

    #[tokio::test]
    async fn manager_is_object_safe_and_drives_every_method() {
        let manager: Arc<dyn Manager> = Arc::new(FakeManager);
        let result = manager
            .deploy(DeployRequest::new("api", "nginx"))
            .await
            .unwrap();
        assert_eq!(result.id, "api-1");
        assert!(manager.status(&result.id).await.unwrap().running);
        assert!(manager.health_check().await.is_ok());
        assert_eq!(
            manager
                .logs(&result.id, LogOptions::default())
                .await
                .unwrap()
                .len(),
            1
        );
        assert!(manager.wait(&result.id).await.is_ok());
        assert!(
            manager
                .list(ListFilter::default())
                .await
                .unwrap()
                .is_empty()
        );
        manager.restart(&result.id).await.unwrap();
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

        let stats: Arc<dyn StatsCapable> = Arc::new(FakeManager);
        assert_eq!(stats.stats("id").await.unwrap().pids, 0);

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

    #[tokio::test]
    async fn optional_runtime_capabilities_are_usable_as_trait_objects() {
        let sysinfo: Arc<dyn SystemInfoCapable> = Arc::new(FakeManager);
        assert_eq!(sysinfo.system_info().await.unwrap().cpus, 8);

        let disk: Arc<dyn DiskUsageCapable> = Arc::new(FakeManager);
        assert_eq!(disk.disk_usage().await.unwrap().images_size, 1024);

        let inspector: Arc<dyn ImageInspector> = Arc::new(FakeManager);
        assert_eq!(
            inspector.image_inspect("nginx:latest").await.unwrap().id,
            "nginx:latest"
        );

        let img_watcher: Arc<dyn ImageEventWatcher> = Arc::new(FakeManager);
        let events: Vec<_> = img_watcher
            .watch_image_events(ImageEventFilter::default())
            .await
            .unwrap()
            .collect()
            .await;
        assert_eq!(events.len(), 1);
    }
}
