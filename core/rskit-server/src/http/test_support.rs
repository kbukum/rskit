use std::time::Duration;

use crate::http_config::HttpServerConfig;

pub(crate) fn local_config() -> HttpServerConfig {
    HttpServerConfig {
        host: "127.0.0.1".to_string(),
        port: 0,
        request_timeout: Duration::from_secs(1),
        ..Default::default()
    }
}
