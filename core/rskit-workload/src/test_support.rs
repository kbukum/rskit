//! Shared test doubles for workload unit tests.
#![allow(clippy::redundant_pub_crate)]

use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use futures::StreamExt;
use futures::stream::BoxStream;
use rskit_errors::{AppError, AppResult, ErrorCode};

use crate::config::WorkloadConfig;
use crate::manager::{
    DiskUsageCapable, EventWatcher, ExecCapable, ImageEventWatcher, ImageInspector, LogStreamer,
    Manager, StatsCapable, SystemInfoCapable,
};
use crate::registry::ManagerFactory;
use crate::report::{
    DeployResult, DiskUsage, ExecResult, ImageConfig, ImageDetail, ImageEvent, SystemInfo,
    WaitResult, WorkloadEvent, WorkloadInfo, WorkloadStats, WorkloadStatus,
};
use crate::spec::{DeployRequest, ImageEventFilter, ListFilter, LogOptions};
use crate::state::WorkloadState;

/// A no-op [`Manager`] that satisfies every core method and capability trait.
pub(crate) struct FakeManager;

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
impl StatsCapable for FakeManager {
    async fn stats(&self, _id: &str) -> AppResult<WorkloadStats> {
        Ok(WorkloadStats::default())
    }
}

#[async_trait]
impl LogStreamer for FakeManager {
    async fn stream_logs(
        &self,
        _id: &str,
        _options: LogOptions,
    ) -> AppResult<BoxStream<'static, AppResult<String>>> {
        Ok(futures::stream::iter(vec![Ok("a".to_string()), Ok("b".to_string())]).boxed())
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

#[async_trait]
impl SystemInfoCapable for FakeManager {
    async fn system_info(&self) -> AppResult<SystemInfo> {
        Ok(SystemInfo {
            os: "linux".into(),
            cpus: 8,
            ..Default::default()
        })
    }
}

#[async_trait]
impl DiskUsageCapable for FakeManager {
    async fn disk_usage(&self) -> AppResult<DiskUsage> {
        Ok(DiskUsage {
            images_size: 1024,
            ..Default::default()
        })
    }
}

#[async_trait]
impl ImageInspector for FakeManager {
    async fn image_inspect(&self, reference: &str) -> AppResult<ImageDetail> {
        Ok(ImageDetail {
            id: reference.to_string(),
            repo_tags: vec![reference.to_string()],
            repo_digests: Vec::new(),
            size: 4096,
            architecture: "amd64".into(),
            os: "linux".into(),
            created: Utc::now(),
            config: ImageConfig::default(),
            layers: Vec::new(),
        })
    }
}

#[async_trait]
impl ImageEventWatcher for FakeManager {
    async fn watch_image_events(
        &self,
        _filter: ImageEventFilter,
    ) -> AppResult<BoxStream<'static, AppResult<ImageEvent>>> {
        let event = ImageEvent {
            action: "pull".into(),
            image_ref: "nginx:latest".into(),
            image_id: "sha256:abc".into(),
            timestamp: Utc::now(),
        };
        Ok(futures::stream::iter(vec![Ok(event)]).boxed())
    }
}

/// A [`ManagerFactory`] that yields a [`FakeManager`].
pub(crate) struct FakeFactory;

#[async_trait]
impl ManagerFactory for FakeFactory {
    async fn create(&self, _config: &WorkloadConfig) -> AppResult<Arc<dyn Manager>> {
        Ok(Arc::new(FakeManager))
    }
}

/// A [`ManagerFactory`] that always fails to build a manager.
pub(crate) struct FailingFactory;

#[async_trait]
impl ManagerFactory for FailingFactory {
    async fn create(&self, _config: &WorkloadConfig) -> AppResult<Arc<dyn Manager>> {
        Err(AppError::new(ErrorCode::Internal, "backend unavailable"))
    }
}

/// A [`Manager`] that builds successfully but reports its backend unreachable.
pub(crate) struct UnhealthyManager;

#[async_trait]
impl Manager for UnhealthyManager {
    async fn deploy(&self, _request: DeployRequest) -> AppResult<DeployResult> {
        Err(AppError::service_unavailable("workload"))
    }
    async fn stop(&self, _id: &str) -> AppResult<()> {
        Err(AppError::service_unavailable("workload"))
    }
    async fn remove(&self, _id: &str) -> AppResult<()> {
        Err(AppError::service_unavailable("workload"))
    }
    async fn restart(&self, _id: &str) -> AppResult<()> {
        Err(AppError::service_unavailable("workload"))
    }
    async fn status(&self, _id: &str) -> AppResult<WorkloadStatus> {
        Err(AppError::service_unavailable("workload"))
    }
    async fn wait(&self, _id: &str) -> AppResult<WaitResult> {
        Err(AppError::service_unavailable("workload"))
    }
    async fn logs(&self, _id: &str, _options: LogOptions) -> AppResult<Vec<String>> {
        Err(AppError::service_unavailable("workload"))
    }
    async fn list(&self, _filter: ListFilter) -> AppResult<Vec<WorkloadInfo>> {
        Err(AppError::service_unavailable("workload"))
    }
    async fn health_check(&self) -> AppResult<()> {
        Err(AppError::service_unavailable("workload"))
    }
}

/// A [`ManagerFactory`] that yields an [`UnhealthyManager`].
pub(crate) struct UnhealthyFactory;

#[async_trait]
impl ManagerFactory for UnhealthyFactory {
    async fn create(&self, _config: &WorkloadConfig) -> AppResult<Arc<dyn Manager>> {
        Ok(Arc::new(UnhealthyManager))
    }
}
