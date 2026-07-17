//! Slash-command registry for interactive agent sessions.
//!
//! Commands are prefixed with `/` and dispatched through a [`CommandRegistry`].
//! Call [`register_builtins`] to register the default `/help`, `/clear`,
//! `/model`, and `/compact` commands.

mod builtins;
mod registry;

pub use builtins::register_builtins;
pub use registry::{Command, CommandHandler, CommandRegistry};

#[cfg(test)]
mod tests;
