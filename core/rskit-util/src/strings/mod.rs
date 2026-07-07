//! String manipulation and casing helpers.

mod case;
mod resolve;
mod suggest;
mod truncate;

pub use case::{to_camel_case, to_kebab_case, to_snake_case};
pub use resolve::{Ambiguity, resolve_unique};
pub use suggest::{DEFAULT_SUGGESTION_DISTANCE, nearest, nearest_within};
pub use truncate::{truncate, truncate_owned};
