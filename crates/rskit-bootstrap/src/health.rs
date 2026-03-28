/// Liveness state of a component.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HealthStatus {
    Healthy,
    Degraded,
    Unhealthy,
}

impl std::fmt::Display for HealthStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HealthStatus::Healthy => f.write_str("healthy"),
            HealthStatus::Degraded => f.write_str("degraded"),
            HealthStatus::Unhealthy => f.write_str("unhealthy"),
        }
    }
}

/// Health report from a single component.
#[derive(Debug, Clone)]
pub struct Health {
    pub name: String,
    pub status: HealthStatus,
    pub message: Option<String>,
}

impl Health {
    pub fn healthy(name: impl Into<String>) -> Self {
        Self { name: name.into(), status: HealthStatus::Healthy, message: None }
    }

    pub fn degraded(name: impl Into<String>, msg: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: HealthStatus::Degraded,
            message: Some(msg.into()),
        }
    }

    pub fn unhealthy(name: impl Into<String>, msg: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: HealthStatus::Unhealthy,
            message: Some(msg.into()),
        }
    }

    pub fn is_healthy(&self) -> bool {
        self.status == HealthStatus::Healthy
    }
}
