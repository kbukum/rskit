//! Low-level collection transformation utilities.

mod chunk;
mod group;
mod index;
mod partition;
mod unique;

pub use chunk::{chunk, chunk_owned};
pub use group::group_by;
pub use index::index_by;
pub use partition::partition;
pub use unique::{ensure_unique_by, find_duplicates_by};
