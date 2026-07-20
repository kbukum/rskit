//! The prompt kinds: one behavioural module per question type.
//!
//! Each kind implements the same question three ways from one place:
//!
//! - **non-interactive** — resolve to the declared default or return a typed error,
//!   without touching the terminal;
//! - **line-driven** — print a numbered list (or plain prompt) once and parse a typed answer,
//!   re-asking on invalid input;
//! - **key-driven** — draw a live frame and update it in place as arrow keys move focus
//!   and space toggles selection.
//!
//! A kind chooses the interactive path by inspecting [`Terminal::capabilities`](crate::prompt::Terminal::capabilities),
//! so the prompter never branches on a concrete terminal type.

mod ask;
pub mod confirm;
mod errors;
pub mod multi_select;
mod navigation;
mod output;
mod raw_mode;
pub mod select;
#[cfg(test)]
mod tests;
pub mod text;

pub(crate) use ask::Ask;
pub(crate) use errors::{cancelled, closed_input, non_interactive_error};
pub(crate) use navigation::{focus_down, focus_up, parse_index};
pub(crate) use output::{notice, write_answer};
pub(crate) use raw_mode::with_raw_mode;
