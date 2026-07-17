//! In-memory cache adapter.

mod cache;
mod registration;

pub use cache::MemoryCache;
pub use registration::register_memory;

#[cfg(test)]
mod tests;
