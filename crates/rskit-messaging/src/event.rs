//! CloudEvents-inspired event envelope for domain events.

use chrono::{DateTime, Utc};
use rskit_errors::{AppError, AppResult, ErrorCode};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// A structured event envelope following CloudEvents conventions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    /// Unique event identifier.
    pub id: String,
    /// The type of event (e.g. `"content.created"`).
    #[serde(rename = "type")]
    pub event_type: String,
    /// The originating service or component.
    pub source: String,
    /// Optional subject the event relates to.
    #[serde(default)]
    pub subject: String,
    /// MIME type of the `data` field.
    #[serde(default = "default_content_type")]
    pub content_type: String,
    /// Schema version of the event.
    #[serde(default)]
    pub version: String,
    /// When the event was created.
    pub timestamp: DateTime<Utc>,
    /// Arbitrary event payload.
    pub data: serde_json::Value,
}

fn default_content_type() -> String {
    "application/json".to_string()
}

impl Event {
    /// Create a new event with the given type and source.
    pub fn new(event_type: impl Into<String>, source: impl Into<String>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            event_type: event_type.into(),
            source: source.into(),
            subject: String::new(),
            content_type: "application/json".to_string(),
            version: "1.0".to_string(),
            timestamp: Utc::now(),
            data: serde_json::Value::Null,
        }
    }

    /// Set the subject.
    pub fn with_subject(mut self, subject: impl Into<String>) -> Self {
        self.subject = subject.into();
        self
    }

    /// Serialize `data` into the event payload.
    pub fn with_data(mut self, data: impl Serialize) -> AppResult<Self> {
        self.data = serde_json::to_value(data).map_err(|e| {
            AppError::new(
                ErrorCode::Internal,
                format!("Failed to serialize event data: {e}"),
            )
        })?;
        Ok(self)
    }

    /// Set the schema version.
    pub fn with_version(mut self, version: impl Into<String>) -> Self {
        self.version = version.into();
        self
    }

    /// Set the content type.
    pub fn with_content_type(mut self, content_type: impl Into<String>) -> Self {
        self.content_type = content_type.into();
        self
    }

    /// Serialize the entire event to JSON bytes.
    pub fn to_json(&self) -> AppResult<Vec<u8>> {
        serde_json::to_vec(self).map_err(|e| {
            AppError::new(
                ErrorCode::Internal,
                format!("Failed to serialize event: {e}"),
            )
        })
    }

    /// Deserialize an event from JSON bytes.
    pub fn from_json(bytes: &[u8]) -> AppResult<Self> {
        serde_json::from_slice(bytes).map_err(|e| {
            AppError::new(
                ErrorCode::Internal,
                format!("Failed to deserialize event: {e}"),
            )
        })
    }

    /// Parse the `data` field into a concrete type.
    pub fn parse_data<T: serde::de::DeserializeOwned>(&self) -> AppResult<T> {
        serde_json::from_value(self.data.clone()).map_err(|e| {
            AppError::new(
                ErrorCode::Internal,
                format!("Failed to parse event data: {e}"),
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_event_has_defaults() {
        let event = Event::new("user.created", "auth-service");
        assert_eq!(event.event_type, "user.created");
        assert_eq!(event.source, "auth-service");
        assert!(!event.id.is_empty());
        assert_eq!(event.content_type, "application/json");
        assert_eq!(event.version, "1.0");
        assert_eq!(event.data, serde_json::Value::Null);
    }

    #[test]
    fn builder_methods() {
        let event = Event::new("order.placed", "shop")
            .with_subject("order-123")
            .with_version("2.0")
            .with_content_type("text/plain");

        assert_eq!(event.subject, "order-123");
        assert_eq!(event.version, "2.0");
        assert_eq!(event.content_type, "text/plain");
    }

    #[test]
    fn with_data_serializes_payload() {
        #[derive(Serialize, Deserialize, Debug, PartialEq)]
        struct Payload {
            name: String,
            count: u32,
        }

        let payload = Payload {
            name: "test".to_string(),
            count: 42,
        };
        let event = Event::new("test.event", "test")
            .with_data(&payload)
            .unwrap();

        let parsed: Payload = event.parse_data().unwrap();
        assert_eq!(parsed, payload);
    }

    #[test]
    fn json_round_trip() {
        let event = Event::new("round.trip", "test-source")
            .with_subject("sub")
            .with_data(serde_json::json!({"key": "value"}))
            .unwrap();

        let bytes = event.to_json().unwrap();
        let restored = Event::from_json(&bytes).unwrap();

        assert_eq!(restored.id, event.id);
        assert_eq!(restored.event_type, "round.trip");
        assert_eq!(restored.source, "test-source");
        assert_eq!(restored.subject, "sub");
        assert_eq!(restored.data, serde_json::json!({"key": "value"}));
    }

    #[test]
    fn serde_renames_type_field() {
        let event = Event::new("check.rename", "src");
        let json = serde_json::to_string(&event).unwrap();
        assert!(json.contains(r#""type":"check.rename""#));
        assert!(!json.contains("event_type"));
    }
}
