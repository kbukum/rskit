/// Liveness state of a component.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
#[non_exhaustive]
pub enum HealthStatus {
    /// Component is operating normally.
    Healthy,
    /// Component is functional but operating in a reduced capacity.
    Degraded,
    /// Component is not functioning correctly.
    Unhealthy,
}

impl std::fmt::Display for HealthStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Healthy => f.write_str("healthy"),
            Self::Degraded => f.write_str("degraded"),
            Self::Unhealthy => f.write_str("unhealthy"),
        }
    }
}

/// Health report from a single component.
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
pub struct Health {
    /// Component name as returned by [`crate::Component::name`].
    pub name: String,
    /// Overall health status of the component.
    pub status: HealthStatus,
    /// Optional human-readable explanation for non-healthy status.
    pub message: Option<String>,
}

impl Health {
    /// Create a healthy report for the named component.
    #[must_use]
    pub fn healthy(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: HealthStatus::Healthy,
            message: None,
        }
    }

    /// Create a degraded report with an explanatory message.
    #[must_use]
    pub fn degraded(name: impl Into<String>, msg: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: HealthStatus::Degraded,
            message: Some(msg.into()),
        }
    }

    /// Create an unhealthy report with an explanatory message.
    #[must_use]
    pub fn unhealthy(name: impl Into<String>, msg: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: HealthStatus::Unhealthy,
            message: Some(msg.into()),
        }
    }

    /// Returns `true` if the status is [`HealthStatus::Healthy`].
    #[must_use]
    pub fn is_healthy(&self) -> bool {
        self.status == HealthStatus::Healthy
    }
}

#[cfg(test)]
mod tests {
    use super::{Health, HealthStatus};

    #[test]
    fn healthy_sets_status_and_no_message() {
        let h = Health::healthy("db");
        assert_eq!(h.status, HealthStatus::Healthy);
        assert_eq!(h.name, "db");
        assert!(h.message.is_none());
    }

    #[test]
    fn healthy_is_healthy_returns_true() {
        assert!(Health::healthy("cache").is_healthy());
    }

    #[test]
    fn degraded_sets_status_and_message() {
        let h = Health::degraded("queue", "high latency");
        assert_eq!(h.status, HealthStatus::Degraded);
        assert_eq!(h.message, Some("high latency".to_string()));
    }

    #[test]
    fn unhealthy_sets_status_and_message() {
        let h = Health::unhealthy("db", "connection refused");
        assert_eq!(h.status, HealthStatus::Unhealthy);
        assert_eq!(h.message, Some("connection refused".to_string()));
    }

    #[test]
    fn health_status_display_values() {
        assert_eq!(HealthStatus::Healthy.to_string(), "healthy");
        assert_eq!(HealthStatus::Degraded.to_string(), "degraded");
        assert_eq!(HealthStatus::Unhealthy.to_string(), "unhealthy");
    }
}
