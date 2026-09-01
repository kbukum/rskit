//! Optional self-description capabilities for components.
//!
//! [`Component`](crate::Component) covers the lifecycle contract (start, stop, health).
//! A component may additionally implement [`Describable`] to report a one-line startup
//! summary, and a server component may implement [`RouteProvider`] to report the HTTP
//! routes it serves. Both are optional metadata capabilities the bootstrap layer consumes
//! to render a startup summary; neither affects lifecycle ordering or health.

/// One-line summary a component self-reports for the startup display.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Description {
    /// Human-readable display name (for example `"HTTP Server"` or `"PostgreSQL"`).
    ///
    /// When empty, callers fall back to [`Component::name`](crate::Component::name).
    pub name: String,
    /// Category of the component, such as `"database"`, `"server"`, or `"cache"`.
    pub kind: String,
    /// One-line human-readable configuration detail shown in the startup summary,
    /// for example `"localhost:5432 pool=25/5"`.
    pub details: String,
    /// Primary port, or `0` when not applicable.
    pub port: u16,
}

impl Description {
    /// Create a description with the given display `name` and all other fields empty.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            ..Self::default()
        }
    }

    /// Set the component category.
    #[must_use]
    pub fn with_kind(mut self, kind: impl Into<String>) -> Self {
        self.kind = kind.into();
        self
    }

    /// Set the one-line configuration detail.
    #[must_use]
    pub fn with_details(mut self, details: impl Into<String>) -> Self {
        self.details = details.into();
        self
    }

    /// Set the primary port.
    #[must_use]
    pub const fn with_port(mut self, port: u16) -> Self {
        self.port = port;
        self
    }
}

/// Optionally implemented by a [`Component`](crate::Component) to self-report startup
/// summary information for the bootstrap display.
pub trait Describable {
    /// Return the startup summary for this component.
    fn describe(&self) -> Description;
}

/// A single HTTP route reported for the startup summary.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Route {
    /// HTTP method (for example `"GET"`).
    pub method: String,
    /// Request path (for example `"/healthz"`).
    pub path: String,
    /// Human-readable handler identifier.
    pub handler: String,
}

impl Route {
    /// Create a route from its `method`, `path`, and `handler` identifier.
    #[must_use]
    pub fn new(
        method: impl Into<String>,
        path: impl Into<String>,
        handler: impl Into<String>,
    ) -> Self {
        Self {
            method: method.into(),
            path: path.into(),
            handler: handler.into(),
        }
    }
}

/// Optionally implemented by a server [`Component`](crate::Component) to auto-report the
/// HTTP routes it registers for the startup summary.
pub trait RouteProvider {
    /// Return the routes this component serves.
    fn routes(&self) -> Vec<Route>;
}

#[cfg(test)]
mod tests {
    use super::{Describable, Description, Route, RouteProvider};

    struct Server;

    impl Describable for Server {
        fn describe(&self) -> Description {
            Description::new("HTTP Server")
                .with_kind("server")
                .with_details("localhost:8080")
                .with_port(8080)
        }
    }

    impl RouteProvider for Server {
        fn routes(&self) -> Vec<Route> {
            vec![Route::new("GET", "/healthz", "health")]
        }
    }

    #[test]
    fn describe_reports_configured_summary() {
        let description = Server.describe();
        assert_eq!(description.name, "HTTP Server");
        assert_eq!(description.kind, "server");
        assert_eq!(description.details, "localhost:8080");
        assert_eq!(description.port, 8080);
    }

    #[test]
    fn description_defaults_are_empty() {
        let description = Description::default();
        assert!(description.name.is_empty());
        assert!(description.kind.is_empty());
        assert!(description.details.is_empty());
        assert_eq!(description.port, 0);
    }

    #[test]
    fn route_provider_reports_registered_routes() {
        let routes = Server.routes();
        assert_eq!(routes, vec![Route::new("GET", "/healthz", "health")]);
    }
}
