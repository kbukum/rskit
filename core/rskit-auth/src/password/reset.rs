use std::time::Duration;

use base64::Engine;
use chrono::{DateTime, Utc};
use rand::RngCore;

/// Generates short-lived opaque reset tokens (random bytes, base64-URL encoded).
#[derive(Debug, Clone)]
pub struct ResetTokenGenerator {
    /// How long the generated token is valid.
    pub ttl: Duration,
}

impl ResetTokenGenerator {
    /// Create a new generator with the given TTL.
    pub const fn new(ttl: Duration) -> Self {
        Self { ttl }
    }

    /// Generate a random token and its expiry time.
    ///
    /// Returns `(token_string, expires_at)`.
    pub fn generate(&self) -> (String, DateTime<Utc>) {
        let mut bytes = [0u8; 32];
        rand::rng().fill_bytes(&mut bytes);
        let token = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes);
        let expires_at = Utc::now() + chrono::Duration::from_std(self.ttl).unwrap_or_default();
        (token, expires_at)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_unique_tokens() {
        let generator = ResetTokenGenerator::new(Duration::from_mins(5));
        let (t1, _) = generator.generate();
        let (t2, _) = generator.generate();
        assert_ne!(t1, t2);
    }

    #[test]
    fn expiry_is_in_the_future() {
        let generator = ResetTokenGenerator::new(Duration::from_mins(5));
        let (_, exp) = generator.generate();
        assert!(exp > Utc::now());
    }
}
