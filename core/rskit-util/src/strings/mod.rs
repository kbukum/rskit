//! String manipulation and casing helpers.

mod case;
mod truncate;

pub use case::{to_camel_case, to_kebab_case, to_snake_case};
pub use truncate::{truncate, truncate_owned};
