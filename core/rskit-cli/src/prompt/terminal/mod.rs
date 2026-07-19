//! The interactive medium a [`Prompter`](crate::prompt::Prompter) speaks through.
//!
//! A `Terminal` abstracts *how* a prompt reads input and renders — decoupled from *what* a prompt asks
//! — so one set of prompt-kind logic drives three very different realities:
//!
//! - [`mod@line`] — [`LineTerminal`]: plain cooked stdio (type a line, press Enter); no raw mode,
//!   works over pipes, dependency-free. The always-available default.
//! - `rich` — `RichTerminal`:
//!   a raw-mode terminal (behind the `interactive` feature) that reads individual [`Key`](crate::prompt::Key)s
//!   so widgets can offer arrow-key navigation with live-highlighted radio and checkbox lists.
//! - [`mod@scripted`] — [`ScriptedTerminal`]: a deterministic test double that feeds canned keys
//!   or lines and captures rendered output, so both the key-driven
//!   and line-driven paths are unit-testable without a real terminal.
//!
//! A terminal advertises whether it is key-driven via [`Capabilities`];
//! prompt kinds branch on that once and never touch a concrete terminal type.

mod capabilities;
pub mod line;

#[cfg(feature = "interactive")]
pub mod rich;

pub mod scripted;
#[cfg(test)]
mod tests;
mod traits;

pub use capabilities::Capabilities;
pub use line::LineTerminal;
#[cfg(feature = "interactive")]
pub use rich::RichTerminal;
pub use scripted::ScriptedTerminal;
pub use traits::Terminal;
