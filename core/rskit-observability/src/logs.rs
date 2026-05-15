use std::time::Duration;

use serde::Deserialize;

use rskit_errors::{AppError, AppResult, ErrorCode};

use crate::tracer::{OtlpProtocol, SERVICE_NAME};

/// Configuration for an OpenTelemetry log pipeline.
#[derive(Debug, Clone, Deserialize)]
pub struct LogsConfig {
    /// Logical service name attached to every log record.
    pub service_name: String,
    /// OTLP collector endpoint. When `None`, logs are not exported.
    pub otlp_endpoint: Option<String>,
    /// Export transport protocol.
    pub protocol: OtlpProtocol,
    /// Maximum time to wait when flushing logs on shutdown.
    pub export_timeout: Duration,
}

/// Injectable OpenTelemetry logger provider handle.
#[derive(Clone)]
pub struct LogsHandle {
    provider: opentelemetry_sdk::logs::SdkLoggerProvider,
}

impl LogsHandle {
    /// Return the SDK logger provider for constructor injection.
    #[must_use]
    pub fn provider(&self) -> opentelemetry_sdk::logs::SdkLoggerProvider {
        self.provider.clone()
    }

    /// Flush and shut down the provider.
    pub fn shutdown(&self) -> AppResult<()> {
        self.provider
            .shutdown()
            .map_err(|e| AppError::new(ErrorCode::Internal, format!("OTLP log shutdown: {e}")))
    }
}

/// Build an injectable OpenTelemetry log provider without touching global state.
///
/// When `otlp_endpoint` is `None`, the provider is built without exporters and
/// acts as an explicit no-export pipeline.
pub fn init_logs(cfg: &LogsConfig) -> AppResult<LogsHandle> {
    use opentelemetry::KeyValue;
    use opentelemetry_otlp::{LogExporter, WithExportConfig};
    use opentelemetry_sdk::{Resource, logs::SdkLoggerProvider};

    let resource = Resource::builder_empty()
        .with_attributes([KeyValue::new(SERVICE_NAME, cfg.service_name.clone())])
        .build();
    let mut builder = SdkLoggerProvider::builder().with_resource(resource);

    if let Some(endpoint) = &cfg.otlp_endpoint {
        let exporter = match cfg.protocol {
            OtlpProtocol::Grpc => LogExporter::builder()
                .with_tonic()
                .with_endpoint(endpoint)
                .with_timeout(cfg.export_timeout)
                .build(),
            OtlpProtocol::HttpBinary => LogExporter::builder()
                .with_http()
                .with_protocol(opentelemetry_otlp::Protocol::HttpBinary)
                .with_endpoint(endpoint)
                .with_timeout(cfg.export_timeout)
                .build(),
        }
        .map_err(|e| AppError::new(ErrorCode::Internal, format!("OTLP log exporter: {e}")))?;
        builder = builder.with_batch_exporter(exporter);
    }

    Ok(LogsHandle {
        provider: builder.build(),
    })
}
