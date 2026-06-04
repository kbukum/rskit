//! Fluent field-level validator — collects all field errors before returning.
//!
//! # Example
//!
//! ```rust
//! use rskit_validation::Validator;
//!
//! fn validate_user(name: &str, email: &str) -> rskit_errors::AppResult<()> {
//!     Validator::new()
//!         .required("name", name)
//!         .max_length("name", name, 100)
//!         .email("email", email)
//!         .validate()
//! }
//! ```

#![warn(missing_docs)]

mod builder;
mod field;
mod rules;

pub mod input;

pub use ::validator::{self, Validate};
pub use builder::Validator;
pub use field::FieldError;
pub use rules::{validate_email, validate_url, validate_uuid};
