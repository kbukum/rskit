//! Conversation memory — store and retrieve message history per session.
//!
//! The [`Memory`] trait defines async operations for loading, saving, and appending messages.
//! [`InMemoryStore`] keeps everything in a `parking_lot::RwLock<HashMap>`,
//! while [`SlidingWindowMemory`] wraps any `Memory` and trims to a maximum message count.

mod sliding_window;
mod store;
mod traits;

pub use sliding_window::SlidingWindowMemory;
pub use store::InMemoryStore;
pub use traits::Memory;

#[cfg(test)]
mod tests;
