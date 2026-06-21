use serde::{Deserialize, Serialize};

/// Deployment environment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[non_exhaustive]
#[serde(rename_all = "lowercase")]
pub enum Environment {
    /// Local development environment (default).
    #[default]
    Development,
    /// Pre-production / staging environment.
    Staging,
    /// Live production environment.
    Production,
}

impl Environment {
    /// Returns `true` if this is the production environment.
    pub fn is_production(&self) -> bool {
        *self == Environment::Production
    }
}

impl std::fmt::Display for Environment {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Environment::Development => f.write_str("development"),
            Environment::Staging => f.write_str("staging"),
            Environment::Production => f.write_str("production"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn environment_display_development() {
        assert_eq!(Environment::Development.to_string(), "development");
    }

    #[test]
    fn environment_display_staging() {
        assert_eq!(Environment::Staging.to_string(), "staging");
    }

    #[test]
    fn environment_display_production() {
        assert_eq!(Environment::Production.to_string(), "production");
    }

    #[test]
    fn environment_default_is_development() {
        assert_eq!(Environment::default(), Environment::Development);
    }

    #[test]
    fn environment_is_production_returns_true_for_production() {
        assert!(Environment::Production.is_production());
    }

    #[test]
    fn environment_is_production_returns_false_for_development() {
        assert!(!Environment::Development.is_production());
    }

    #[test]
    fn environment_is_production_returns_false_for_staging() {
        assert!(!Environment::Staging.is_production());
    }

    #[test]
    fn environment_deserialize_from_lowercase_string() {
        let dev: Environment = serde_json::from_str(r#""development""#).unwrap();
        assert_eq!(dev, Environment::Development);

        let stg: Environment = serde_json::from_str(r#""staging""#).unwrap();
        assert_eq!(stg, Environment::Staging);

        let prd: Environment = serde_json::from_str(r#""production""#).unwrap();
        assert_eq!(prd, Environment::Production);
    }

    #[test]
    fn environment_deserialize_unknown_string_fails() {
        let result: Result<Environment, _> = serde_json::from_str(r#""unknown""#);
        assert!(result.is_err());
    }

    #[test]
    fn environment_clone_and_eq() {
        let env = Environment::Staging;
        let cloned = env.clone();
        assert_eq!(env, cloned);
    }
}
