//! Agent result types, events, and context strategies.

mod event;
mod limit;
mod result;
mod stop_reason;
mod strategy;

pub use event::AgentEvent;
pub use limit::AgentLimitError;
pub use result::AgentResult;
pub use stop_reason::StopReason;
pub use strategy::{ContextStrategy, FailStrategy, TruncateStrategy};
