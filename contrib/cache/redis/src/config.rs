use std::fmt;
use std::time::Duration;

use rskit_util::SecretString;
use serde::{Deserialize, Serialize};

/// Redis connection and pool configuration.
#[derive(Clone, Deserialize, Serialize)]
pub struct Config {
    /// Redis server hostname or IP address.
    #[serde(default = "default_host")]
    pub host: String,
    /// Redis server port.
    #[serde(default = "default_port")]
    pub port: u16,
    /// Optional password for Redis AUTH.
    pub password: Option<SecretString>,
    /// Redis database index.
    #[serde(default)]
    pub database: u8,
    /// Timeout for establishing a connection.
    #[serde(default = "default_connect_timeout", with = "duration_seconds")]
    pub connect_timeout: Duration,
    /// Timeout applied to each individual Redis command (get/set/delete/exists).
    ///
    /// Bounds every remote call so a stalled connection cannot hang a caller indefinitely.
    #[serde(default = "default_operation_timeout", with = "duration_seconds")]
    pub operation_timeout: Duration,
    /// Optional prefix prepended to every key.
    pub key_prefix: Option<String>,
}

impl fmt::Debug for Config {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Config")
            .field("host", &self.host)
            .field("port", &self.port)
            .field("password", &self.password.as_ref().map(|_| "<redacted>"))
            .field("database", &self.database)
            .field("connect_timeout", &self.connect_timeout)
            .field("operation_timeout", &self.operation_timeout)
            .field("key_prefix", &self.key_prefix)
            .finish()
    }
}

impl Default for Config {
    fn default() -> Self {
        Self {
            host: default_host(),
            port: default_port(),
            password: None,
            database: 0,
            connect_timeout: default_connect_timeout(),
            operation_timeout: default_operation_timeout(),
            key_prefix: None,
        }
    }
}

impl Config {
    pub(crate) fn connection_url(&self) -> String {
        match &self.password {
            Some(pw) => format!(
                "redis://:{}@{}:{}/{}",
                pw.expose(),
                self.host,
                self.port,
                self.database
            ),
            None => format!("redis://{}:{}/{}", self.host, self.port, self.database),
        }
    }
}

fn default_host() -> String {
    "127.0.0.1".to_owned()
}

fn default_port() -> u16 {
    6379
}

fn default_connect_timeout() -> Duration {
    Duration::from_secs(5)
}

fn default_operation_timeout() -> Duration {
    Duration::from_secs(5)
}

mod duration_seconds {
    use std::time::Duration;

    use serde::{Deserialize, Deserializer, Serializer};

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Duration, D::Error>
    where
        D: Deserializer<'de>,
    {
        u64::deserialize(deserializer).map(Duration::from_secs)
    }

    pub fn serialize<S>(duration: &Duration, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u64(duration.as_secs())
    }
}
