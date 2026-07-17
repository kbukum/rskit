//! Agent — the multi-turn agentic execution loop.

mod component;
mod definition;
mod run;
mod stream;

pub use definition::Agent;

#[cfg(test)]
mod tests;
