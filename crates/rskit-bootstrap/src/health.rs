/// Liveness state of a component.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
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
            HealthStatus::Healthy => f.write_str("healthy"),
            HealthStatus::Degraded => f.write_str("degraded"),
            HealthStatus::Unhealthy => f.write_str("unhealthy"),
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
    pub fn healthy(name: impl Into<String>) -> Self {
        Self { name: name.into(), status: HealthStatus::Healthy, message: None }
    }

    /// Create a degraded report with an explanatory message.
    pub fn degraded(name: impl Into<String>, msg: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: HealthStatus::Degraded,
            message: Some(msg.into()),
        }
    }

    /// Create an unhealthy report with an explanatory message.
    pub fn unhealthy(name: impl Into<String>, msg: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            status: HealthStatus::Unhealthy,
            message: Some(msg.into()),
        }
    }

    /// Returns `true` if the status is [`HealthStatus::Healthy`].
    pub fn is_healthy(&self) -> bool {
        self.status == HealthStatus::Healthy
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn healthy_sets_status_and_no_message() {
        let h = Health::healthy("db");
        assert_eq!(h.status, HealthStatus::Healthy);
        assert_eq!(h.name, "db");
        assert!(h.message.is_none());
    }

    #[test]
    fn healthy_is_healthy_returns_true() {
        let h = Health::healthy("cache");
        assert!(h.is_healthy());
    }

    #[test]
    fn degraded_sets_status_and_message() {
        let h = Health::degraded("queue", "high latency");
        assert_eq!(h.status, HealthStatus::Degraded);
        assert_eq!(h.message, Some("high latency".to_string()));
    }

    #[test]
    fn degraded_is_healthy_returns_false() {
        let h = Health::degraded("queue", "slow");
        assert!(!h.is_healthy());
    }

    #[test]
    fn unhealthy_sets_status_and_message() {
        let h = Health::unhealthy("db", "connection refused");
        assert_eq!(h.status, HealthStatus::Unhealthy);
        assert_eq!(h.message, Some("connection refused".to_string()));
    }

    #[test]
    fn unhealthy_is_healthy_returns_false() {
        let h = Health::unhealthy("db", "down");
        assert!(!h.is_healthy());
    }

    #[test]
    fn health_status_display_healthy() {
        assert_eq!(format!("{}", HealthStatus::Healthy), "healthy");
    }

    #[test]
    fn health_status_display_degraded() {
        assert_eq!(format!("{}", HealthStatus::Degraded), "degraded");
    }

    #[test]
    fn health_status_display_unhealthy() {
        assert_eq!(format!("{}", HealthStatus::Unhealthy), "unhealthy");
    }
}
