//! Answer validation with interactive re-ask.
//!
//! A [`Validator`] inspects a candidate answer and either accepts it
//! or returns a short human-readable reason. In an interactive prompt the reason is shown
//! and the question is re-asked;
//! in [`PromptMode::NonInteractive`](crate::prompt::PromptMode::NonInteractive) a rejected default is a typed error rather than a silent bad value.
//!
//! Any `Fn(&str) -> Result<(), String>` is a `Validator`, so callers pass a closure directly;
//! reusable rules can also implement the trait by hand.

/// The outcome of validating a candidate answer: `Ok(())` accepts it,
/// `Err` carries a short reason to display before re-asking.
pub type Validation = Result<(), String>;

/// Inspects a candidate answer, accepting it or explaining why it is rejected.
pub trait Validator {
    /// Validate `input`, returning `Ok(())` to accept or `Err(reason)` to reject.
    ///
    /// # Errors
    ///
    /// Returns the human-readable rejection reason to surface to the user.
    fn validate(&self, input: &str) -> Validation;
}

impl<F> Validator for F
where
    F: Fn(&str) -> Validation,
{
    fn validate(&self, input: &str) -> Validation {
        self(input)
    }
}

/// A validator that rejects empty or whitespace-only input with `message`.
///
/// A convenience for the most common rule; equivalent to a closure that trims and checks for emptiness.
#[must_use]
pub fn non_empty(message: impl Into<String>) -> impl Validator {
    let message = message.into();
    move |input: &str| {
        if input.trim().is_empty() {
            Err(message.clone())
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{Validator, non_empty};

    #[test]
    fn closure_is_a_validator() {
        let v = |input: &str| {
            if input.len() >= 2 {
                Ok(())
            } else {
                Err("too short".to_string())
            }
        };
        assert!(v.validate("ok").is_ok());
        assert_eq!(v.validate("x"), Err("too short".to_string()));
    }

    #[test]
    fn non_empty_rejects_blank() {
        let v = non_empty("required");
        assert!(v.validate("value").is_ok());
        assert_eq!(v.validate("   "), Err("required".to_string()));
    }
}
