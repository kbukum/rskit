//! Message translation between wire format and domain types.

use rskit_errors::{AppError, AppResult, ErrorCode};
use serde::{Serialize, de::DeserializeOwned};

/// Translates messages between wire format and domain types.
///
/// `W` is the wire type carried inside [`Message<W>`](crate::Message) and
/// `D` is the domain type the application works with.
pub trait MessageTranslator<W, D>: Send + Sync + 'static {
    /// Serializes a domain type to wire format.
    fn serialize(&self, domain: &D) -> AppResult<W>;

    /// Deserializes wire format to domain type.
    fn deserialize(&self, raw: &W) -> AppResult<D>;
}

/// JSON translator for `Vec<u8>` ↔ serde-compatible types.
///
/// Serializes domain objects to JSON bytes and deserializes them back.
#[derive(Debug, Clone, Copy, Default)]
pub struct JsonTranslator;

impl<D> MessageTranslator<Vec<u8>, D> for JsonTranslator
where
    D: Serialize + DeserializeOwned + Send + Sync + 'static,
{
    fn serialize(&self, domain: &D) -> AppResult<Vec<u8>> {
        serde_json::to_vec(domain).map_err(|e| {
            AppError::new(
                ErrorCode::Internal,
                format!("JSON serialization failed: {e}"),
            )
        })
    }

    fn deserialize(&self, raw: &Vec<u8>) -> AppResult<D> {
        serde_json::from_slice(raw).map_err(|e| {
            AppError::new(
                ErrorCode::Internal,
                format!("JSON deserialization failed: {e}"),
            )
        })
    }
}

/// JSON translator for `String` ↔ serde-compatible types.
///
/// Serializes domain objects to JSON strings and deserializes them back.
#[derive(Debug, Clone, Copy, Default)]
pub struct JsonStringTranslator;

impl<D> MessageTranslator<String, D> for JsonStringTranslator
where
    D: Serialize + DeserializeOwned + Send + Sync + 'static,
{
    fn serialize(&self, domain: &D) -> AppResult<String> {
        serde_json::to_string(domain).map_err(|e| {
            AppError::new(
                ErrorCode::Internal,
                format!("JSON serialization failed: {e}"),
            )
        })
    }

    fn deserialize(&self, raw: &String) -> AppResult<D> {
        serde_json::from_str(raw).map_err(|e| {
            AppError::new(
                ErrorCode::Internal,
                format!("JSON deserialization failed: {e}"),
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
    struct User {
        name: String,
        age: u32,
    }

    #[test]
    fn json_translator_round_trip() {
        let translator = JsonTranslator;
        let user = User {
            name: "Alice".into(),
            age: 30,
        };

        let bytes: Vec<u8> =
            MessageTranslator::<Vec<u8>, User>::serialize(&translator, &user).unwrap();
        let restored: User =
            MessageTranslator::<Vec<u8>, User>::deserialize(&translator, &bytes).unwrap();
        assert_eq!(restored, user);
    }

    #[test]
    fn json_string_translator_round_trip() {
        let translator = JsonStringTranslator;
        let user = User {
            name: "Bob".into(),
            age: 25,
        };

        let json: String =
            MessageTranslator::<String, User>::serialize(&translator, &user).unwrap();
        assert!(json.contains("Bob"));
        let restored: User =
            MessageTranslator::<String, User>::deserialize(&translator, &json).unwrap();
        assert_eq!(restored, user);
    }

    #[test]
    fn json_translator_bad_input() {
        let translator = JsonTranslator;
        let bad = vec![0xFF, 0xFE];
        let result: AppResult<User> =
            MessageTranslator::<Vec<u8>, User>::deserialize(&translator, &bad);
        assert!(result.is_err());
    }
}
