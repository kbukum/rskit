use std::fmt;
use std::time::Duration;

use rskit_util::SecretString;
use serde::{Deserialize, Serialize};

/// Redis connection and pool configuration.
#[derive(Clone, Serialize, Deserialize)]
#[serde(try_from = "RawConfig", into = "WireConfig")]
pub struct Config {
    /// Redis server hostname or IP address.
    pub host: String,
    /// Redis server port.
    pub port: u16,
    /// Optional password for Redis AUTH.
    pub password: Option<SecretString>,
    /// Redis database index.
    pub database: u8,
    /// Timeout for establishing a connection.
    #[serde(with = "duration_seconds")]
    pub connect_timeout: Duration,
    /// Timeout applied to each individual Redis command (get/set/delete/exists).
    ///
    /// Bounds every remote call so a stalled connection cannot hang a caller indefinitely.
    #[serde(with = "duration_seconds")]
    pub operation_timeout: Duration,
    /// Optional prefix prepended to every key.
    pub key_prefix: Option<String>,
}

/// Raw deserialization shape reconciling the shared `addr`/`db` keys with the
/// typed `host`/`port`/`database` representation. Supplying both `addr` and
/// `host`/`port` is rejected as an ambiguous alias.
#[derive(Deserialize)]
struct RawConfig {
    /// Combined `host:port` address (gokit-shared key).
    addr: Option<String>,
    host: Option<String>,
    port: Option<u16>,
    password: Option<SecretString>,
    #[serde(default, alias = "db")]
    database: u8,
    #[serde(default = "default_connect_timeout", with = "duration_seconds")]
    connect_timeout: Duration,
    #[serde(default = "default_operation_timeout", with = "duration_seconds")]
    operation_timeout: Duration,
    key_prefix: Option<String>,
}

impl TryFrom<RawConfig> for Config {
    type Error = String;

    fn try_from(raw: RawConfig) -> Result<Self, Self::Error> {
        let (host, port) = match raw.addr {
            Some(addr) => {
                if raw.host.is_some() || raw.port.is_some() {
                    return Err(
                        "redis config: specify either `addr` or `host`/`port`, not both".to_owned(),
                    );
                }
                parse_addr(&addr)?
            }
            None => (
                raw.host.unwrap_or_else(default_host),
                raw.port.unwrap_or_else(default_port),
            ),
        };
        Ok(Self {
            host,
            port,
            password: raw.password,
            database: raw.database,
            connect_timeout: raw.connect_timeout,
            operation_timeout: raw.operation_timeout,
            key_prefix: raw.key_prefix,
        })
    }
}

/// Serialize-only wire shape emitting the shared `addr`/`db` keys so the adapter
/// produces the same contract the sibling Redis config consumes, rather than the
/// internal `host`/`port`/`database` fields.
#[derive(Serialize)]
struct WireConfig {
    /// Combined `host:port` address (gokit-shared key).
    addr: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    password: Option<SecretString>,
    /// Redis database index (gokit-shared key).
    db: u8,
    #[serde(with = "duration_seconds")]
    connect_timeout: Duration,
    #[serde(with = "duration_seconds")]
    operation_timeout: Duration,
    #[serde(skip_serializing_if = "Option::is_none")]
    key_prefix: Option<String>,
}

impl From<Config> for WireConfig {
    fn from(config: Config) -> Self {
        Self {
            addr: format!("{}:{}", config.host, config.port),
            password: config.password,
            db: config.database,
            connect_timeout: config.connect_timeout,
            operation_timeout: config.operation_timeout,
            key_prefix: config.key_prefix,
        }
    }
}

fn parse_addr(addr: &str) -> Result<(String, u16), String> {
    let (host, port) = addr
        .rsplit_once(':')
        .ok_or_else(|| format!("redis config: `addr` must be `host:port`, got {addr:?}"))?;
    if host.is_empty() {
        return Err(format!(
            "redis config: `addr` host must not be empty, got {addr:?}"
        ));
    }
    let port = port
        .parse::<u16>()
        .map_err(|error| format!("redis config: invalid `addr` port in {addr:?}: {error}"))?;
    Ok((host.to_owned(), port))
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
